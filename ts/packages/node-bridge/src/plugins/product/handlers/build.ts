import { mkdir, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';

import { asRecord, loadLanConfig, loadPackageJsonSnapshot } from '../../../core/runtime.js';
import {
  discoverManifestPaths,
  normalizeSchemaDiscovery,
  normalizeSchemaEntries,
} from '../../dynamic-commands/parse-manifest.js';

import { computeProductCompatSnapshot } from '../compat.js';
import { copyDirectory, ensureModuleSchemaRoot, writeJsonFile } from '../fs.js';
import { createDistributionReport } from '../report.js';
import type { ProductDistributionReport } from '../types.js';
import { structuredCloneRecord, toPortableRelativePath, uniqueStrings } from '../utils.js';

// `product.build` materializes a "snapshot" directory that contains:
// - copied schema roots (so runtime can load them without depending on workspace layout)
// - rewritten `lan.config.json` and `product.config.json`
// - optional templates directory
// - optional package.json snapshot

export async function handleBuild(params: Record<string, unknown>) {
  const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
  const outputDir =
    typeof params.outputDir === 'string' && params.outputDir.trim().length > 0
      ? params.outputDir.trim()
      : '.lania/build/product';
  const clean = params.clean !== false;

  const loaded = await loadLanConfig(cwd);
  if (!loaded.exists || !loaded.configPath) {
    throw new Error('lan build product requires a local lan.config.* file');
  }

  const productConfig = asRecord(loaded.config.product);
  const schemaConfig = asRecord(loaded.config.schema);
  const discovery = normalizeSchemaDiscovery(schemaConfig.discovery ?? loaded.config.schemaDiscovery);
  const configuredEntries = normalizeSchemaEntries(schemaConfig.entry ?? productConfig.schemaEntry);
  const discovered = await discoverManifestPaths(cwd, discovery, configuredEntries);

  const outputRoot = resolve(cwd, outputDir);
  if (clean) {
    await rm(outputRoot, { recursive: true, force: true });
  }
  await mkdir(outputRoot, { recursive: true });

  const generatedFiles: string[] = [];
  const schemaRoots = uniqueStrings(discovered.paths.map((entry) => dirname(entry)));
  const rootMapping = new Map<string, string>();
  const copiedSchemaEntries: string[] = [];

  // Copy each schema root into a stable location under dist/.
  for (const [index, schemaRoot] of schemaRoots.entries()) {
    const destinationRoot = join(outputRoot, 'dist', 'schema-roots', `root-${index}`);
    await copyDirectory(schemaRoot, destinationRoot, [outputRoot]);
    await ensureModuleSchemaRoot(destinationRoot);
    rootMapping.set(schemaRoot, destinationRoot);
  }

  // Convert manifest paths to point at copied roots.
  for (const manifestPath of discovered.paths) {
    const copiedRoot = rootMapping.get(dirname(manifestPath));
    if (!copiedRoot) {
      continue;
    }
    copiedSchemaEntries.push(
      toPortableRelativePath(outputRoot, join(copiedRoot, basename(manifestPath))),
    );
  }

  const copiedTemplatesDir = await copyTemplatesDir(cwd, outputRoot, productConfig);
  if (copiedTemplatesDir) {
    generatedFiles.push('./templates');
  }

  // Rewrite config to reference copied schema entries and templates directory.
  const rewrittenConfig = rewriteBuiltConfig(loaded.config, copiedSchemaEntries, copiedTemplatesDir);
  await writeJsonFile(join(outputRoot, 'lan.config.json'), rewrittenConfig);
  generatedFiles.push('./lan.config.json');
  await writeJsonFile(join(outputRoot, 'product.config.json'), rewrittenConfig);
  generatedFiles.push('./product.config.json');

  // Persist a package.json snapshot if present; publish flow uses it for defaults.
  const packageJson = await loadPackageJsonSnapshot(cwd);
  const compatSnapshot = await computeProductCompatSnapshot({
    productConfig,
    packageJson,
    hostVersion: typeof params.hostVersion === 'string' ? params.hostVersion : null,
  });
  if (Object.keys(packageJson).length > 0) {
    await writeFile(join(outputRoot, 'package.json'), `${JSON.stringify(packageJson, null, 2)}\n`, 'utf8');
    generatedFiles.push('./package.json');
  }

  const schemaRootOutputs = [...rootMapping.values()].map((entry) => toPortableRelativePath(outputRoot, entry));
  generatedFiles.push(...schemaRootOutputs);

  const report: ProductDistributionReport = createDistributionReport(
    {
      kind: 'product_build',
      mode: 'snapshot',
      outputRoot,
      productRoot: toPortableRelativePath(outputRoot, outputRoot),
      nodeBridgeDir: null,
      wrapper: null,
      tarball: null,
      bundle: null,
      checks: {
        hasConfigPath: true,
        hasSchemaEntries: copiedSchemaEntries.length > 0,
        hasSchemaRoots: schemaRootOutputs.length > 0,
        hasTemplatesDir: copiedTemplatesDir !== null,
        hasPackageJson: Object.keys(packageJson).length > 0,
      },
      generatedFiles,
      experimental: {
        cwd,
        configPath: loaded.configPath,
        schemaEntries: copiedSchemaEntries,
        schemaRoots: schemaRootOutputs,
        templatesDir: copiedTemplatesDir,
        warnings: [...discovered.warnings],
        compat: compatSnapshot,
      },
    },
    'build-report.json',
  );

  await writeJsonFile(join(outputRoot, 'build-report.json'), report);
  return {
    result: report,
    events: [],
  };
}

async function copyTemplatesDir(
  cwd: string,
  outputRoot: string,
  productConfig: Record<string, unknown>,
): Promise<string | null> {
  const templatesDir =
    typeof productConfig.templatesDir === 'string' && productConfig.templatesDir.trim().length > 0
      ? resolve(cwd, productConfig.templatesDir)
      : null;
  if (!templatesDir) {
    return null;
  }

  const destination = join(outputRoot, 'templates');
  try {
    await copyDirectory(templatesDir, destination);
    return './templates';
  } catch {
    // Copy failures are treated as non-fatal because templates are optional for some workflows.
    return null;
  }
}

function rewriteBuiltConfig(
  config: Record<string, unknown>,
  schemaEntries: string[],
  templatesDir: string | null,
): Record<string, unknown> {
  // We clone because `loadLanConfig` returns a shared object which other callers might reuse.
  const nextConfig = structuredCloneRecord(config);
  const schemaConfig = asRecord(nextConfig.schema);
  const productConfig = asRecord(nextConfig.product);

  nextConfig.schema = {
    ...schemaConfig,
    entry: schemaEntries,
  };
  nextConfig.product = {
    ...productConfig,
    ...(templatesDir ? { templatesDir } : {}),
  };

  return nextConfig;
}

