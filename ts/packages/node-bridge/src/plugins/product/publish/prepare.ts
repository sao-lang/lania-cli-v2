import { mkdir, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { asRecord, fileExists } from '../../../core/runtime.js';

import { computeProductCompatSnapshot } from '../compat.js';
import { copyDirectory, readJsonFile, readOptionalJsonFile, writeJsonFile } from '../fs.js';
import { createDistributionReport } from '../report.js';
import {
  resolvePlatformBinarySource,
  runNpmPack,
  stageOfficialCliBundle,
  stagePublishedWrapper,
} from '../staging.js';
import type { ProductPublishManifest } from '../types.js';
import { toPortableRelativePath } from '../utils.js';
import { createPublishManifest } from './manifest.js';
import {
  createPublishedReadme,
  createRegistryManifestPackages,
  resolvePublishAccess,
  resolvePublishChannel,
  resolvePublishDistTag,
  resolvePublishPackageName,
  resolvePublishPackageVersion,
  resolvePublishRegistry,
} from './resolve.js';
import type { NormalizedPublishOptions } from './options.js';
import { repoRoot } from '../constants.js';

// 该函数负责“发布前产物准备”的完整流水线：
// - 校验/读取 pack 阶段输出
// - 生成 publish 目录结构（wrapper、README、package.json、bundle）
// - 生成用于后续执行 publish 的 manifest/report
// 设计上它只产出静态制品与描述文件，不在这里直接执行 npm publish。
export async function preparePublishArtifacts(
  params: Record<string, unknown>,
  options: NormalizedPublishOptions,
) {
  // pack 阶段把 product 与 node-bridge 一并放在 install-root/lib 下，
  // publish 阶段直接复用这两个目录，避免再次构建造成版本不一致。
  const productRoot = join(options.packRoot, 'lib', 'product');
  const nodeBridgeDir = join(options.packRoot, 'lib', 'node-bridge');

  // 这里读取的三个输入分别代表：
  // - packReport: pack 阶段执行记录（用于最终报告透出）
  // - builtConfig: 编译后的产品配置（决定包名、版本、二进制名等）
  // - sourcePackageJson: 产品 package.json（用于版本/兼容信息补全）
  const packReport = await readJsonFile(join(options.packRoot, 'pack-report.json'));
  const builtConfig = await readJsonFile(join(productRoot, 'product.config.json'));
  const sourcePackageJson = await readOptionalJsonFile(join(productRoot, 'package.json'));
  const productConfig = asRecord(builtConfig.product);

  const compatSnapshot = await computeProductCompatSnapshot({
    productConfig,
    packageJson: sourcePackageJson,
    hostVersion: typeof params.hostVersion === 'string' ? params.hostVersion : null,
  });

  const packageName = resolvePublishPackageName(productConfig, sourcePackageJson);
  const packageVersion = resolvePublishPackageVersion(productConfig, sourcePackageJson);
  const platformBinarySource = await resolvePlatformBinarySource(params);
  const binaryName =
    typeof productConfig.binaryName === 'string' && productConfig.binaryName.trim().length > 0
      ? productConfig.binaryName.trim()
      : 'lan';
  const distTag = resolvePublishDistTag(params, packageVersion);
  const channel = resolvePublishChannel(params, distTag);
  const registry = resolvePublishRegistry(params);
  const access = resolvePublishAccess(params);

  // npm/cli 的版本会写入发布包依赖，确保发布产物与当前仓库内 CLI 版本对齐。
  const cliPackage = await readJsonFile(join(repoRoot, 'npm/cli/package.json'));

  // clean=true 时先清空输出目录，再写入全量新产物，避免残留文件污染发布包。
  if (options.clean) {
    await rm(options.outputRoot, { recursive: true, force: true });
  }
  await mkdir(options.outputRoot, { recursive: true });

  await copyDirectory(productRoot, join(options.outputRoot, 'lib', 'product'));
  await copyDirectory(nodeBridgeDir, join(options.outputRoot, 'lib', 'node-bridge'));

  const wrapperPath = await stagePublishedWrapper(options.outputRoot, binaryName);
  const readmePath = join(options.outputRoot, 'README.md');
  await writeFile(
    readmePath,
    createPublishedReadme({
      packageName,
      binaryName,
      version: packageVersion,
    }),
    'utf8',
  );

  const packageJson = {
    name: packageName,
    version: packageVersion,
    private: false,
    type: 'module',
    bin: {
      [binaryName]: `./bin/${binaryName}.mjs`,
    },
    files: ['bin', 'lib', 'README.md'],
    dependencies: {
      '@lania-cli/cli': String(cliPackage.version ?? '0.1.0'),
    },
  };
  await writeJsonFile(join(options.outputRoot, 'package.json'), packageJson);

  // 先执行 npm pack 生成产品包 tarball，再补齐官方 CLI bundle；
  // 两者都会被记录到 report/manifest，供后续 publish 执行与审计使用。
  const tarball = runNpmPack(options.outputRoot);
  const officialBundle = await stageOfficialCliBundle(
    options.outputRoot,
    params,
    platformBinarySource,
  );

  const productTarball = toPortableRelativePath(
    options.outputRoot,
    join(options.outputRoot, tarball.filename),
  );
  const checks = {
    hasPackReport: true,
    hasProductConfig: true,
    hasNodeBridgePayload: await fileExists(join(nodeBridgeDir, 'dist', 'entry', 'stdio.js')),
    hasSchemaEntries:
      Array.isArray(asRecord(builtConfig.schema).entry) &&
      (asRecord(builtConfig.schema).entry as unknown[]).length > 0,
    hasTarball: true,
    hasOfficialCliTarball: officialBundle.cliTarball !== null,
    hasPlatformBinaryTarball: officialBundle.platformTarball !== null,
    hasBundleRoot: true,
  };

  // registryPackages 是“将要发布到 registry 的包清单”，
  // 会被同时写入 report.experimental.registryPublish 与 publish-manifest，
  // 保证执行前（计划）与执行时（动作）读取的是同一套声明。
  const registryPackages = createRegistryManifestPackages({
    packageName,
    packageVersion,
    productTarball,
    cliTarball: officialBundle.cliTarball,
    cliPackageVersion: String(cliPackage.version ?? '0.1.0'),
    platformTarballs: officialBundle.platformTarballs,
    distTag,
    channel,
  });

  const report = createDistributionReport(
    {
      kind: 'product_publish',
      mode: 'npm_package',
      outputRoot: options.outputRoot,
      wrapper: toPortableRelativePath(options.outputRoot, wrapperPath),
      tarball: productTarball,
      bundle: officialBundle,
      productRoot: './lib/product',
      nodeBridgeDir: './lib/node-bridge',
      checks,
      generatedFiles: [
        './package.json',
        './README.md',
        './bin',
        './lib/product',
        './lib/node-bridge',
        './bundle',
        './publish-manifest.json',
        ...(officialBundle.cliTarball ? [officialBundle.cliTarball] : []),
        ...officialBundle.platformTarballs.map((entry) => entry.tarball),
        productTarball,
      ],
      experimental: {
        cwd: options.cwd,
        packRoot: options.packRoot,
        packageName,
        packageVersion,
        binaryName,
        packReport,
        compat: compatSnapshot,
        platformBinarySource: platformBinarySource
          ? {
              packageName: platformBinarySource.packageName,
              packageRoot: platformBinarySource.packageRoot,
              platform: platformBinarySource.platform,
              binaryPath: platformBinarySource.binaryPath,
              source: platformBinarySource.source,
            }
          : null,
        registryPublish: {
          manifest: './publish-manifest.json',
          distTag,
          channel,
          registry,
          access,
          packages: registryPackages,
        },
      },
    },
    'publish-report.json',
  );

  const manifestPath = join(options.outputRoot, 'publish-manifest.json');
  const reportPath = join(options.outputRoot, 'publish-report.json');
  // manifest 面向“执行器”，report 面向“可读审计信息”。
  // 两者都来源于同一批中间结果，避免各自独立计算导致信息漂移。
  const manifest = createPublishManifest({
    outputRoot: options.outputRoot,
    packageName,
    packageVersion,
    binaryName,
    distTag,
    channel,
    registry,
    access,
    productTarball,
    bundleRoot: report.bundle?.root ?? null,
    packages: registryPackages,
    platformMatrix: report.bundle?.platformMatrix ?? [],
    checks: report.checks,
  });

  return {
    manifest,
    manifestPath,
    report,
    reportPath,
    registryPackages,
  };
}
