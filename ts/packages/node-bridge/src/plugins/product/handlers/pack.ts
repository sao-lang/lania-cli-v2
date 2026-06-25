import { mkdir, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';

import { asRecord, fileExists, loadPackageJsonSnapshot } from '../../../core/runtime.js';

import { computeProductCompatSnapshot } from '../compat.js';
import { copyDirectory, readJsonFile, writeJsonFile } from '../fs.js';
import { createDistributionReport } from '../report.js';
import { stageNodeBridgePayload, stageProductWrapper } from '../staging.js';
import { toPortableRelativePath } from '../utils.js';

// `product.pack` assembles an "install-root" layout:
// - `lib/product` is the previously built snapshot
// - `lib/node-bridge` is a staged runtime payload (dist + deps)
// - `bin/<binary>` is a wrapper that re-invokes the host CLI with env overrides

export async function handlePack(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  const buildDir =
    typeof params.buildDir === 'string' && params.buildDir.trim().length > 0
      ? params.buildDir.trim()
      : '.lania/build/product';
  const outputDir =
    typeof params.outputDir === 'string' && params.outputDir.trim().length > 0
      ? params.outputDir.trim()
      : '.lania/pack/product/install-root';
  const clean = params.clean !== false;

  const buildRoot = resolve(cwd, buildDir);
  const outputRoot = resolve(cwd, outputDir);

  const builtConfig = await readJsonFile(join(buildRoot, 'product.config.json'));
  const buildReport = await readJsonFile(join(buildRoot, 'build-report.json'));
  const productConfig = asRecord(builtConfig.product);

  const packageJson = await loadPackageJsonSnapshot(cwd);
  const compatSnapshot = await computeProductCompatSnapshot({
    productConfig,
    packageJson,
    hostVersion: typeof params.hostVersion === 'string' ? params.hostVersion : null,
  });

  const binaryName =
    typeof productConfig.binaryName === 'string' && productConfig.binaryName.trim().length > 0
      ? productConfig.binaryName.trim()
      : 'lan';

  if (clean) {
    await rm(outputRoot, { recursive: true, force: true });
  }
  await mkdir(outputRoot, { recursive: true });

  const libRoot = join(outputRoot, 'lib');
  const productTarget = join(libRoot, 'product');
  const nodeBridgeTarget = join(libRoot, 'node-bridge');

  // Copy build snapshot as-is; it already contains rewritten configs and copied schema roots.
  await copyDirectory(buildRoot, productTarget);

  // Stage node-bridge payload for installed execution.
  await stageNodeBridgePayload(nodeBridgeTarget);

  // Create wrapper executable.
  const wrapperPath = await stageProductWrapper(outputRoot, binaryName);

  const report = createDistributionReport(
    {
      kind: 'product_pack',
      mode: 'install_root',
      outputRoot,
      wrapper: toPortableRelativePath(outputRoot, wrapperPath),
      productRoot: './lib/product',
      nodeBridgeDir: './lib/node-bridge',
      tarball: null,
      bundle: null,
      checks: {
        hasBuildReport: true,
        hasProductConfig: await fileExists(join(productTarget, 'product.config.json')),
        hasNodeBridgePayload: await fileExists(join(nodeBridgeTarget, 'dist', 'entry', 'stdio.js')),
        hasWrapper: await fileExists(wrapperPath),
      },
      generatedFiles: [
        toPortableRelativePath(outputRoot, wrapperPath),
        './lib/product',
        './lib/node-bridge',
      ],
      experimental: {
        cwd,
        buildRoot,
        binaryName,
        buildReport,
        compat: compatSnapshot,
      },
    },
    'pack-report.json',
  );

  await writeJsonFile(join(outputRoot, 'pack-report.json'), report);
  return {
    result: report,
    events: [],
  };
}

