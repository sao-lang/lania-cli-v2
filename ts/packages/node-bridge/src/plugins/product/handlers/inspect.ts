import { dirname, join, resolve } from 'node:path';

import { asRecord, fileExists, loadLanConfig, loadPackageJsonSnapshot } from '../../../core/runtime.js';
import {
  discoverManifestPaths,
  normalizeSchemaDiscovery,
  normalizeSchemaEntries,
} from '../../dynamic-commands/parse-manifest.js';

import { COMPAT_REPORT_VERSION, PRODUCT_REPORT_VERSION } from '../constants.js';
import { computeProductCompatSnapshot } from '../compat.js';
import { writeJsonFile } from '../fs.js';
import type { ProductCompatSummary, ProductInspectReport } from '../types.js';
import { firstNonEmptyString, toPortableRelativePath, uniqueStrings } from '../utils.js';

// `product.inspect` reports local product state and optionally emits a compat report.
//
// It is designed to be safe to run in any directory:
// - It only requires a local `lan.config.*` to exist.
// - It never mutates the workspace except for writing reports under `.lania/...` when requested.

export async function handleInspect(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  const doctorMode = params.doctor === true;

  const loaded = await loadLanConfig(cwd);
  if (!loaded.exists || !loaded.configPath) {
    throw new Error('lan inspect product requires a local lan.config.* file');
  }

  const productConfig = asRecord(loaded.config.product);
  const schemaConfig = asRecord(loaded.config.schema);
  const discovery = normalizeSchemaDiscovery(schemaConfig.discovery ?? loaded.config.schemaDiscovery);
  const configuredEntries = normalizeSchemaEntries(schemaConfig.entry ?? productConfig.schemaEntry);
  const discovered = await discoverManifestPaths(cwd, discovery, configuredEntries);

  const packageJson = await loadPackageJsonSnapshot(cwd);

  // Convention-based artifact locations used by build/pack/publish handlers.
  const buildDir = resolve(cwd, '.lania/build/product');
  const packDir = resolve(cwd, '.lania/pack/product/install-root');
  const publishDir = resolve(cwd, '.lania/publish/product/npm-package');

  const runtimeMode =
    firstNonEmptyString(params.runtimeMode, process.env.LANIA_RUNTIME_MODE) === 'installed'
      ? 'installed'
      : 'development';

  const schemaRoots = uniqueStrings(discovered.paths.map((entry) => dirname(entry)));
  const templatesDir =
    typeof productConfig.templatesDir === 'string' && productConfig.templatesDir.trim().length > 0
      ? productConfig.templatesDir.trim()
      : null;

  const hasBuildReport = await fileExists(join(buildDir, 'build-report.json'));
  const hasPackReport = await fileExists(join(packDir, 'pack-report.json'));
  const hasPublishReport = await fileExists(join(publishDir, 'publish-report.json'));

  const checks = {
    hasConfigPath: true,
    hasSchemaEntries: discovered.paths.length > 0,
    hasTemplatesDir: templatesDir !== null,
    hasBuildReport,
    hasPackReport,
    hasPublishReport,
  };

  const nextSteps = buildProductInspectNextSteps({
    checks,
    warnings: discovered.warnings,
    runtimeMode,
  });

  // Compat is optional because it may write an extra report file.
  const wantsCompat = params.compat === true;
  let compat: ProductCompatSummary | undefined;
  if (wantsCompat) {
    const snapshot = await computeProductCompatSnapshot({
      productConfig,
      packageJson,
      hostVersion: typeof params.hostVersion === 'string' ? params.hostVersion : null,
    });
    const reportPath = join(cwd, '.lania', 'inspect', 'product', 'compat-report.json');
    const report = {
      reportVersion: COMPAT_REPORT_VERSION,
      kind: 'compat_report',
      generatedAt: new Date().toISOString(),
      cwd,
      verdict: snapshot.verdict,
      reasons: snapshot.reasons,
      declared: snapshot.declared,
      actual: snapshot.actual,
      product: snapshot.product,
    };
    await writeJsonFile(reportPath, report);
    compat = {
      verdict: snapshot.verdict,
      reasons: snapshot.reasons,
      reportPath: toPortableRelativePath(cwd, reportPath),
      declared: snapshot.declared,
      actual: report.actual,
      product: report.product,
    };
  }

  const result: ProductInspectReport = {
    accepted: true,
    reportVersion: PRODUCT_REPORT_VERSION,
    kind: 'product_inspect',
    mode: runtimeMode,
    ...(doctorMode ? { doctor: true } : {}),
    cwd,
    configPath: loaded.configPath,
    checks,
    ...(compat ? { compat } : {}),
    product: {
      name: typeof productConfig.name === 'string' ? productConfig.name : null,
      binaryName: typeof productConfig.binaryName === 'string' ? productConfig.binaryName : null,
      displayName: typeof productConfig.displayName === 'string' ? productConfig.displayName : null,
      templatesDir,
    },
    schema: {
      entries: discovered.paths.map((entry) => toPortableRelativePath(cwd, entry)),
      roots: schemaRoots.map((entry) => toPortableRelativePath(cwd, entry)),
      warnings: [...discovered.warnings],
    },
    artifacts: {
      buildDir: toPortableRelativePath(cwd, buildDir),
      hasBuildReport,
      packDir: toPortableRelativePath(cwd, packDir),
      hasPackReport,
      publishDir: toPortableRelativePath(cwd, publishDir),
      hasPublishReport,
    },
    packageJson: {
      name: typeof packageJson.name === 'string' ? packageJson.name : null,
      version: typeof packageJson.version === 'string' ? packageJson.version : null,
    },
    nextSteps,
    exitCode: 0,
  };

  return {
    result,
    events: [],
  };
}

function buildProductInspectNextSteps(input: {
  checks: Record<string, boolean>;
  warnings: string[];
  runtimeMode: 'development' | 'installed';
}): string[] {
  // These strings are intentionally command-like; they show up in `lan product inspect` output.
  const steps: string[] = [];
  if (!input.checks.hasSchemaEntries) {
    steps.push(
      'Add `schema.entry` in lan.config.* or place product schema files under the configured discovery roots.',
    );
  }
  if (!input.checks.hasTemplatesDir) {
    steps.push(
      'Set `product.templatesDir` in lan.config.* and create the template directory for product authoring.',
    );
  }
  if (!input.checks.hasBuildReport) {
    steps.push('Run `lan product build` to materialize the local product snapshot.');
  }
  if (input.checks.hasBuildReport && !input.checks.hasPackReport) {
    steps.push('Run `lan product pack` to prepare the install-root artifact for product validation.');
  }
  if (input.checks.hasPackReport && !input.checks.hasPublishReport) {
    steps.push(
      'Run `lan product publish --dry-run` to verify the publish plan before registry delivery.',
    );
  }
  if (input.warnings.length > 0) {
    steps.push(
      'Review the schema discovery warnings above and align lan.config.* with the actual product layout.',
    );
  }
  if (input.runtimeMode === 'development') {
    steps.push(
      'Use `lan product dev --watch <command>` while iterating on local product commands.',
    );
  }
  return uniqueStrings(steps);
}
