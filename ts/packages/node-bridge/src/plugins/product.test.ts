import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path, { dirname } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { productPlugin } from './product.js';

const repoRoot = path.resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..', '..');

test('product.generate scaffolds a minimal product workspace', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-generate-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const handled = await productPlugin.handle('product.generate', {
    cwd,
    name: 'Acme CLI',
    binaryName: 'acme',
  });
  assert.ok(handled);

  const result = handled.result as {
    accepted: boolean;
    reportVersion: number;
    kind: string;
    mode: string;
    checks: Record<string, boolean>;
    generatedFiles: string[];
    experimental?: {
      packageName?: string;
      binaryName?: string;
      displayName?: string;
    };
  };
  assert.equal(result.accepted, true);
  assert.equal(result.reportVersion, 1);
  assert.equal(result.kind, 'product_generate');
  assert.equal(result.mode, 'scaffold');
  assert.equal(result.experimental?.packageName, '@lania-product/acme');
  assert.equal(result.experimental?.binaryName, 'acme');
  assert.equal(result.experimental?.displayName, 'Acme CLI');
  assert.equal(result.checks.hasLanConfig, true);
  assert.equal(result.checks.hasSchemaEntry, true);
  assert.ok(result.generatedFiles.includes('./lan.config.json'));
  assert.ok(result.generatedFiles.includes('./product/lania.schemas.ts'));

  const outputRoot = path.join(cwd, 'products', 'acme-cli');
  const packageJson = JSON.parse(await readFile(path.join(outputRoot, 'package.json'), 'utf8')) as {
    name: string;
    private: boolean;
  };
  assert.equal(packageJson.name, '@lania-product/acme');
  assert.equal(packageJson.private, true);

  const lanConfig = JSON.parse(await readFile(path.join(outputRoot, 'lan.config.json'), 'utf8')) as {
    product?: {
      binaryName?: string;
      templatesDir?: string;
      compat?: {
        frameworkVersionRange?: string;
        protocolVersionRange?: string;
        nodeBridgeVersionRange?: string;
      };
    };
    schema?: { entry?: string };
  };
  assert.equal(lanConfig.product?.binaryName, 'acme');
  assert.equal(lanConfig.product?.templatesDir, './product/templates');
  assert.equal(lanConfig.product?.compat?.frameworkVersionRange, '>=0.1.0 <1.0.0');
  assert.equal(lanConfig.product?.compat?.protocolVersionRange, '0.1.0');
  assert.equal(lanConfig.product?.compat?.nodeBridgeVersionRange, '0.1.0');
  assert.equal(lanConfig.schema?.entry, './product/lania.schemas.ts');

  const schemaSource = await readFile(path.join(outputRoot, 'product', 'lania.schemas.ts'), 'utf8');
  assert.match(schemaSource, /hello from Acme CLI/);
  assert.match(schemaSource, /name: 'hello'/);
});

test('product.inspect reports config, schema, and local product artifacts', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-inspect-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        displayName: 'Acme CLI',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  const handled = await productPlugin.handle('product.inspect', { cwd });
  assert.ok(handled);

  const result = handled.result as {
    accepted: boolean;
    reportVersion: number;
    kind: string;
    mode: string;
    checks: Record<string, boolean>;
    product: { name: string | null; binaryName: string | null; templatesDir: string | null };
    schema: { entries: string[]; roots: string[] };
    artifacts: { hasBuildReport: boolean; hasPackReport: boolean; buildDir: string };
    packageJson: { name: string | null; version: string | null };
    nextSteps: string[];
  };
  assert.equal(result.accepted, true);
  assert.equal(result.reportVersion, 1);
  assert.equal(result.kind, 'product_inspect');
  assert.equal(result.mode, 'development');
  assert.equal(result.product.name, '@acme/demo');
  assert.equal(result.product.binaryName, 'acme');
  assert.equal(result.product.templatesDir, './product/templates');
  assert.equal(result.packageJson.name, '@acme/demo');
  assert.equal(result.packageJson.version, '1.2.3');
  assert.equal(result.checks.hasSchemaEntries, true);
  assert.equal(result.checks.hasTemplatesDir, true);
  assert.equal(result.artifacts.hasBuildReport, true);
  assert.equal(result.artifacts.hasPackReport, false);
  assert.equal(result.artifacts.buildDir, '.lania/build/product');
  assert.deepEqual(result.schema.entries, ['./product/lania.schemas.ts']);
  assert.deepEqual(result.schema.roots, ['./product']);
  assert.ok(
    result.nextSteps.includes(
      'Run `lan product pack` to prepare the install-root artifact for product validation.',
    ),
  );
  assert.ok(
    result.nextSteps.includes(
      'Use `lan product dev --watch <command>` while iterating on local product commands.',
    ),
  );
});

test('product.inspect --compat writes compat-report and returns summary', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-inspect-compat-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        displayName: 'Acme CLI',
        templatesDir: './product/templates',
        compat: {
          frameworkVersionRange: '>=0.1.0 <1.0.0',
          protocolVersionRange: '0.1.0',
          nodeBridgeVersionRange: '0.1.0',
          productVersionRange: '^1.2.3'
        }
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );

  const handled = await productPlugin.handle('product.inspect', {
    cwd,
    compat: true,
    hostVersion: '0.1.0',
  });
  assert.ok(handled);

  const result = handled.result as {
    compat?: {
      verdict: string;
      reportPath: string;
      declared?: Record<string, string | null>;
      actual: { hostVersion: string | null; protocolVersion: string; nodeBridgeVersion: string | null };
      product: { productVersion: string | null };
    };
  };
  assert.equal(result.compat?.verdict, 'ready');
  assert.equal(result.compat?.reportPath, '.lania/inspect/product/compat-report.json');
  assert.equal(result.compat?.declared?.protocolVersionRange, '0.1.0');
  assert.equal(result.compat?.actual.hostVersion, '0.1.0');
  assert.equal(result.compat?.actual.protocolVersion, '0.1.0');
  assert.equal(result.compat?.product.productVersion, '1.2.3');

  const compatReport = JSON.parse(
    await readFile(path.join(cwd, '.lania', 'inspect', 'product', 'compat-report.json'), 'utf8'),
  ) as {
    kind: string;
    verdict: string;
    declared?: Record<string, string | null>;
    actual: { hostVersion: string | null };
  };
  assert.equal(compatReport.kind, 'compat_report');
  assert.equal(compatReport.verdict, 'ready');
  assert.equal(compatReport.declared?.frameworkVersionRange, '>=0.1.0 <1.0.0');
  assert.equal(compatReport.actual.hostVersion, '0.1.0');
});

test('product.inspect --compat warns on incompatible declared ranges', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-inspect-compat-warn-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        compat: {
          frameworkVersionRange: '>=9.9.9'
        }
      },
      schema: { entry: './product/lania.schemas.ts' }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );

  const handled = await productPlugin.handle('product.inspect', {
    cwd,
    compat: true,
    hostVersion: '0.1.0',
  });
  assert.ok(handled);
  const result = handled.result as { compat?: { verdict: string; reasons: string[] } };
  assert.equal(result.compat?.verdict, 'warn');
  assert.ok(result.compat?.reasons.some((reason) => reason.includes('frameworkVersionRange')));
});

test('product.build creates a minimal product snapshot output', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product', 'workflows'), { recursive: true });
  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        versionStrategy: 'package_json',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default {
      commands: [{ name: 'create-app', workflow: 'createApp' }],
      workflows: {
        createApp: async () => undefined
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'workflows', 'create-app.ts'),
    'export const marker = true;\n',
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  const handled = await productPlugin.handle('product.build', {
    cwd,
    outputDir: '.lania/build/product',
  });
  assert.ok(handled);

  const result = handled.result as {
    accepted: boolean;
    reportVersion: number;
    kind: string;
    mode: string;
    productRoot: string | null;
    nodeBridgeDir: string | null;
    wrapper: string | null;
    tarball: string | null;
    bundle: unknown;
    checks: Record<string, boolean>;
    generatedFiles: string[];
    experimental?: {
      schemaEntries?: string[];
      templatesDir?: string | null;
      compat?: {
        actual?: { protocolVersion?: string };
      };
    };
  };
  assert.equal(result.accepted, true);
  assert.equal(result.reportVersion, 1);
  assert.equal(result.kind, 'product_build');
  assert.equal(result.mode, 'snapshot');
  assert.equal(result.productRoot, './');
  assert.equal(result.nodeBridgeDir, null);
  assert.equal(result.wrapper, null);
  assert.equal(result.tarball, null);
  assert.equal(result.bundle, null);
  assert.equal(result.checks.hasSchemaEntries, true);
  assert.equal(result.checks.hasTemplatesDir, true);
  assert.deepEqual(result.experimental?.schemaEntries, ['./dist/schema-roots/root-0/lania.schemas.ts']);
  assert.equal(result.experimental?.templatesDir, './templates');
  assert.equal(result.experimental?.compat?.actual?.protocolVersion, '0.1.0');
  assert.ok(result.generatedFiles.includes('./lan.config.json'));
  assert.ok(result.generatedFiles.includes('./product.config.json'));
  assert.ok(result.generatedFiles.includes('./package.json'));
  assert.ok(result.generatedFiles.includes('./templates'));
  assert.ok(result.generatedFiles.includes('./dist/schema-roots/root-0'));
  assert.ok(result.generatedFiles.includes('./build-report.json'));

  const outputRoot = path.join(cwd, '.lania', 'build', 'product');
  const builtConfig = JSON.parse(await readFile(path.join(outputRoot, 'lan.config.json'), 'utf8')) as {
    schema?: { entry?: string[] };
    product?: { templatesDir?: string };
  };
  assert.deepEqual(builtConfig.schema?.entry, ['./dist/schema-roots/root-0/lania.schemas.ts']);
  assert.equal(builtConfig.product?.templatesDir, './templates');

  const copiedManifest = await readFile(
    path.join(outputRoot, 'dist', 'schema-roots', 'root-0', 'lania.schemas.ts'),
    'utf8',
  );
  assert.match(copiedManifest, /create-app/);

  const copiedTemplate = JSON.parse(
    await readFile(path.join(outputRoot, 'templates', 'base', 'template.json'), 'utf8'),
  ) as { id: string };
  assert.equal(copiedTemplate.id, 'base');

  const copiedPackageJson = JSON.parse(
    await readFile(path.join(outputRoot, 'package.json'), 'utf8'),
  ) as { version: string };
  assert.equal(copiedPackageJson.version, '1.2.3');
});

test('product.pack creates a minimal install-root layout from product build output', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-pack-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  const packed = await productPlugin.handle('product.pack', { cwd });
  assert.ok(packed);

  const result = packed.result as {
    accepted: boolean;
    reportVersion: number;
    kind: string;
    mode: string;
    tarball: string | null;
    bundle: unknown;
    wrapper: string;
    productRoot: string;
    nodeBridgeDir: string;
    checks: Record<string, boolean>;
    generatedFiles: string[];
    experimental?: {
      binaryName?: string;
      compat?: {
        actual?: { protocolVersion?: string };
      };
    };
  };
  assert.equal(result.accepted, true);
  assert.equal(result.reportVersion, 1);
  assert.equal(result.kind, 'product_pack');
  assert.equal(result.mode, 'install_root');
  assert.equal(result.experimental?.binaryName, 'acme');
  assert.equal(result.experimental?.compat?.actual?.protocolVersion, '0.1.0');
  assert.equal(result.wrapper, './bin/acme');
  assert.equal(result.productRoot, './lib/product');
  assert.equal(result.nodeBridgeDir, './lib/node-bridge');
  assert.equal(result.tarball, null);
  assert.equal(result.bundle, null);
  assert.equal(result.checks.hasBuildReport, true);
  assert.equal(result.checks.hasNodeBridgePayload, true);
  assert.equal(result.checks.hasWrapper, true);

  const installRoot = path.join(cwd, '.lania', 'pack', 'product', 'install-root');
  const wrapper = await readFile(path.join(installRoot, 'bin', 'acme'), 'utf8');
  assert.match(wrapper, /LANIA_NODE_BRIDGE_DIR/);
  assert.match(wrapper, /LANIA_PRODUCT_ROOT/);
  assert.match(wrapper, /LANIA_RUNTIME_MODE/);

  const copiedBuildReport = JSON.parse(
    await readFile(path.join(installRoot, 'lib', 'product', 'build-report.json'), 'utf8'),
  ) as { kind: string };
  assert.equal(copiedBuildReport.kind, 'product_build');

  const copiedBridgePackage = JSON.parse(
    await readFile(path.join(installRoot, 'lib', 'node-bridge', 'package.json'), 'utf8'),
  ) as { dependencies?: Record<string, string> };
  assert.equal(typeof copiedBridgePackage.dependencies?.tsx, 'string');
  assert.equal(typeof copiedBridgePackage.dependencies?.yaml, 'string');

  const packReport = JSON.parse(
    await readFile(path.join(installRoot, 'pack-report.json'), 'utf8'),
  ) as {
    kind: string;
    generatedFiles: string[];
    reportVersion: number;
    experimental?: { compat?: { actual?: { protocolVersion?: string } } };
  };
  assert.equal(packReport.kind, 'product_pack');
  assert.equal(packReport.reportVersion, 1);
  assert.equal(packReport.experimental?.compat?.actual?.protocolVersion, '0.1.0');
  assert.ok(packReport.generatedFiles.includes('./bin/acme'));
  assert.ok(packReport.generatedFiles.includes('./pack-report.json'));
});

test('product.publish creates a publish-ready npm package layout from packed output', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-publish-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));
  const officialStagingBinary = path.join(repoRoot, 'npm', 'cli-darwin-arm64', 'bin', 'lania-cli');
  const originalStagingBinary = await readFile(officialStagingBinary).catch(() => null);
  t.after(async () => {
    if (originalStagingBinary) {
      await writeFile(officialStagingBinary, originalStagingBinary);
      return;
    }
    await rm(officialStagingBinary, { force: true });
  });
  await rm(officialStagingBinary, { force: true });

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  await productPlugin.handle('product.pack', { cwd });
  const published = await productPlugin.handle('product.publish', { cwd });
  assert.ok(published);

  const result = published.result as unknown as {
    accepted: boolean;
    reportVersion: number;
    kind: string;
    mode: string;
    wrapper: string;
    tarball: string;
    bundle: {
      root: string;
      cliTarball: string | null;
      platformTarball: string | null;
      platformTarballs: Array<{
        packageName: string;
        platform: string;
        tarball: string;
        source: string;
      }>;
    };
    productRoot: string;
    nodeBridgeDir: string;
    checks: {
      hasPackReport: boolean;
      hasProductConfig: boolean;
      hasNodeBridgePayload: boolean;
      hasSchemaEntries: boolean;
      hasTarball: boolean;
      hasOfficialCliTarball: boolean;
      hasPlatformBinaryTarball: boolean;
    };
    experimental?: {
      packageName?: string;
      packageVersion?: string;
      binaryName?: string;
      compat?: {
        actual?: { protocolVersion?: string };
      };
    };
  };
  assert.equal(result.accepted, true);
  assert.equal(result.reportVersion, 1);
  assert.equal(result.kind, 'product_publish');
  assert.equal(result.mode, 'npm_package');
  assert.equal(result.experimental?.packageName, '@acme/demo');
  assert.equal(result.experimental?.packageVersion, '1.2.3');
  assert.equal(result.experimental?.binaryName, 'acme');
  assert.equal(result.experimental?.compat?.actual?.protocolVersion, '0.1.0');
  assert.equal(result.wrapper, './bin/acme.mjs');
  assert.match(result.tarball, /^\.\/.*\.tgz$/);
  assert.equal(result.bundle.root, './bundle');
  assert.equal(result.bundle.cliTarball, './bundle/cli-package/lania-cli-cli-0.1.0.tgz');
  assert.equal(result.bundle.platformTarball, null);
  assert.deepEqual(result.bundle.platformTarballs, []);
  assert.equal(result.productRoot, './lib/product');
  assert.equal(result.nodeBridgeDir, './lib/node-bridge');
  assert.equal(result.checks.hasPackReport, true);
  assert.equal(result.checks.hasProductConfig, true);
  assert.equal(result.checks.hasNodeBridgePayload, true);
  assert.equal(result.checks.hasSchemaEntries, true);
  assert.equal(result.checks.hasTarball, true);
  assert.equal(result.checks.hasOfficialCliTarball, true);
  assert.equal(result.checks.hasPlatformBinaryTarball, false);

  const publishRoot = path.join(cwd, '.lania', 'publish', 'product', 'npm-package');
  const packageJson = JSON.parse(
    await readFile(path.join(publishRoot, 'package.json'), 'utf8'),
  ) as {
    name: string;
    version: string;
    bin: Record<string, string>;
    dependencies: Record<string, string>;
  };
  assert.equal(packageJson.name, '@acme/demo');
  assert.equal(packageJson.version, '1.2.3');
  assert.equal(packageJson.bin.acme, './bin/acme.mjs');
  assert.equal(typeof packageJson.dependencies['@lania-cli/cli'], 'string');

  const wrapper = await readFile(path.join(publishRoot, 'bin', 'acme.mjs'), 'utf8');
  assert.match(wrapper, /@lania-cli\/cli/);
  assert.match(wrapper, /LANIA_PRODUCT_ROOT/);
  assert.match(wrapper, /LANIA_NODE_BRIDGE_DIR/);

  const publishReport = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-report.json'), 'utf8'),
  ) as {
    kind: string;
    reportVersion: number;
    tarball: string;
    bundle: {
      root: string;
      cliTarball: string | null;
      platformTarball: string | null;
      platformTarballs: Array<unknown>;
      platformMatrix: Array<{
        packageName: string;
        platform: string;
        version: string;
        status: string;
        tarball: string | null;
        source: string;
      }>;
    };
    generatedFiles: string[];
    experimental?: { compat?: { actual?: { protocolVersion?: string } } };
  };
  assert.equal(publishReport.kind, 'product_publish');
  assert.equal(publishReport.reportVersion, 1);
  assert.match(publishReport.tarball, /^\.\/.*\.tgz$/);
  assert.equal(publishReport.bundle.root, './bundle');
  assert.equal(publishReport.bundle.cliTarball, './bundle/cli-package/lania-cli-cli-0.1.0.tgz');
  assert.equal(publishReport.bundle.platformTarball, null);
  assert.equal(publishReport.experimental?.compat?.actual?.protocolVersion, '0.1.0');
  assert.deepEqual(publishReport.bundle.platformTarballs, []);
  assert.deepEqual(publishReport.bundle.platformMatrix, [
    {
      packageName: '@lania-cli/cli-darwin-arm64',
      platform: 'darwin-arm64',
      version: '0.1.0',
      status: 'binary_missing',
      tarball: null,
      source: 'package_dir',
    },
    {
      packageName: '@lania-cli/cli-linux-x64',
      platform: 'linux-x64',
      version: '0.1.0',
      status: 'binary_missing',
      tarball: null,
      source: 'package_dir',
    },
  ]);
  assert.ok(publishReport.generatedFiles.includes('./lib/product'));
  assert.ok(publishReport.generatedFiles.includes('./lib/node-bridge'));
  assert.ok(publishReport.generatedFiles.includes('./bundle'));
  assert.ok(publishReport.generatedFiles.includes('./publish-manifest.json'));
  assert.ok(publishReport.generatedFiles.includes('./publish-report.json'));
  assert.ok(publishReport.generatedFiles.includes('./bundle/cli-package/lania-cli-cli-0.1.0.tgz'));
  assert.ok(
    publishReport.generatedFiles.some((entry) => typeof entry === 'string' && entry.endsWith('.tgz')),
  );

  const publishManifest = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-manifest.json'), 'utf8'),
  ) as {
    kind: string;
    mode: string;
    distTag: string;
    channel: string;
    productTarball: string;
    bundleRoot: string | null;
    packages: Array<{ role: string; name: string; tarball: string; publishStrategy: string }>;
    platformMatrix: Array<{
      packageName: string;
      platform: string;
      version: string;
      status: string;
      tarball: string | null;
      source: string;
    }>;
    publishOrder: string[];
    dependencyLinks: Array<{ from: string; to: string; type: string; field: string }>;
    steps: Array<{
      id: string;
      packageName: string;
      role: string;
      tarball: string;
      distTag: string;
      dependsOn: string[];
      publishConfig: {
        registry: string;
        access: string;
        otpRequired: string;
        provenance: boolean;
        dryRun: boolean;
      };
      command: { program: string; args: string[] };
    }>;
  };
  assert.equal(publishManifest.kind, 'product_publish_manifest');
  assert.equal(publishManifest.mode, 'registry_plan');
  assert.equal(publishManifest.distTag, 'latest');
  assert.equal(publishManifest.channel, 'latest');
  assert.match(publishManifest.productTarball, /^\.\/.*\.tgz$/);
  assert.equal(publishManifest.bundleRoot, './bundle');
  assert.deepEqual(
    publishManifest.packages.map((entry) => entry.role),
    ['product', 'official_cli'],
  );
  assert.equal(publishManifest.packages[0]?.name, '@acme/demo');
  assert.equal(publishManifest.packages[1]?.name, '@lania-cli/cli');
  assert.deepEqual(
    publishManifest.packages.map((entry) => entry.publishStrategy),
    ['npm_tarball_publish', 'npm_tarball_publish'],
  );
  assert.deepEqual(publishManifest.platformMatrix, [
    {
      packageName: '@lania-cli/cli-darwin-arm64',
      platform: 'darwin-arm64',
      version: '0.1.0',
      status: 'binary_missing',
      tarball: null,
      source: 'package_dir',
    },
    {
      packageName: '@lania-cli/cli-linux-x64',
      platform: 'linux-x64',
      version: '0.1.0',
      status: 'binary_missing',
      tarball: null,
      source: 'package_dir',
    },
  ]);
  assert.deepEqual(publishManifest.publishOrder, ['@lania-cli/cli', '@acme/demo']);
  assert.deepEqual(publishManifest.dependencyLinks, [
    {
      from: '@acme/demo',
      to: '@lania-cli/cli',
      type: 'dependency',
      field: 'dependencies',
    },
  ]);
  assert.deepEqual(
    publishManifest.steps.map((entry) => entry.packageName),
    ['@lania-cli/cli', '@acme/demo'],
  );
  assert.equal(publishManifest.steps[0]?.command.program, 'npm');
  assert.deepEqual(publishManifest.steps[0]?.command.args.slice(0, 6), [
    'publish',
    './bundle/cli-package/lania-cli-cli-0.1.0.tgz',
    '--tag',
    'latest',
    '--access',
    'public',
  ]);
  assert.equal(publishManifest.steps[0]?.command.args[6], '--registry');
  assert.equal(
    publishManifest.steps[0]?.command.args[7],
    publishManifest.steps[0]?.publishConfig.registry,
  );
  assert.equal(publishManifest.steps[0]?.publishConfig.access, 'public');
  assert.equal(publishManifest.steps[0]?.publishConfig.otpRequired, 'unknown');
  assert.equal(publishManifest.steps[0]?.publishConfig.provenance, false);
  assert.equal(publishManifest.steps[0]?.publishConfig.dryRun, false);
  assert.deepEqual(publishManifest.steps[1]?.dependsOn, ['@lania-cli/cli']);
});

test('product.publish stages linux-x64 bundle from explicit env binary source', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-publish-linux-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const linuxBinarySource = path.join(cwd, 'fixtures', 'lania-cli-linux-x64');
  await mkdir(path.dirname(linuxBinarySource), { recursive: true });
  await writeFile(linuxBinarySource, '#!/bin/sh\nexit 0\n', 'utf8');

  const previousLinuxEnv = process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64;
  process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64 = linuxBinarySource;
  t.after(() => {
    if (previousLinuxEnv === undefined) {
      delete process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64;
      return;
    }
    process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64 = previousLinuxEnv;
  });

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  await productPlugin.handle('product.pack', { cwd });
  const published = await productPlugin.handle('product.publish', { cwd });
  assert.ok(published);

  const publishRoot = path.join(cwd, '.lania', 'publish', 'product', 'npm-package');
  const publishReport = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-report.json'), 'utf8'),
  ) as {
    bundle: {
      platformTarballs: Array<{ packageName: string; platform: string; tarball: string; source: string }>;
      platformMatrix: Array<{
        packageName: string;
        platform: string;
        status: string;
        tarball: string | null;
        source: string;
      }>;
    };
  };
  const publishManifest = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-manifest.json'), 'utf8'),
  ) as {
    packages: Array<{
      role: string;
      name: string;
      tarball: string;
      optional?: boolean;
      platform?: string;
      source?: string;
    }>;
    publishOrder: string[];
    dependencyLinks: Array<{ from: string; to: string; type: string; field: string }>;
    steps: Array<{
      packageName: string;
      role: string;
      tarball: string;
      dependsOn: string[];
    }>;
  };

  const linuxTarball = publishReport.bundle.platformTarballs.find(
    (entry) => entry.packageName === '@lania-cli/cli-linux-x64',
  );
  assert.ok(linuxTarball);
  assert.equal(linuxTarball.platform, 'linux-x64');
  assert.equal(linuxTarball.source, 'environment');
  assert.match(linuxTarball.tarball, /^\.\/bundle\/cli-linux-x64-package\/.*\.tgz$/);

  const linuxMatrix = publishReport.bundle.platformMatrix.find(
    (entry) => entry.packageName === '@lania-cli/cli-linux-x64',
  );
  assert.deepEqual(linuxMatrix, {
    packageName: '@lania-cli/cli-linux-x64',
    platform: 'linux-x64',
    version: '0.1.0',
    status: 'ready',
    tarball: linuxTarball.tarball,
    source: 'environment',
  });

  const linuxPackage = publishManifest.packages.find(
    (entry) => entry.name === '@lania-cli/cli-linux-x64',
  );
  assert.equal(linuxPackage?.role, 'platform_binary');
  assert.equal(linuxPackage?.name, '@lania-cli/cli-linux-x64');
  assert.equal(linuxPackage?.tarball, linuxTarball.tarball);
  assert.equal(linuxPackage?.optional, true);
  assert.equal(linuxPackage?.platform, 'linux-x64');
  assert.equal(linuxPackage?.source, 'environment');
  assert.ok(publishManifest.publishOrder.includes('@lania-cli/cli-linux-x64'));
  assert.ok(
    publishManifest.publishOrder.indexOf('@lania-cli/cli-linux-x64') <
      publishManifest.publishOrder.indexOf('@lania-cli/cli'),
  );
  assert.ok(
    publishManifest.dependencyLinks.some(
      (entry) =>
        entry.from === '@lania-cli/cli' &&
        entry.to === '@lania-cli/cli-linux-x64' &&
        entry.type === 'optional_dependency' &&
        entry.field === 'optionalDependencies',
    ),
  );
  const linuxStep = publishManifest.steps.find(
    (entry) => entry.packageName === '@lania-cli/cli-linux-x64',
  );
  assert.equal(linuxStep?.packageName, '@lania-cli/cli-linux-x64');
  assert.equal(linuxStep?.role, 'platform_binary');
  assert.equal(linuxStep?.tarball, linuxTarball.tarball);
  assert.deepEqual(linuxStep?.dependsOn, []);
});

test('product.publish stages linux-x64 bundle from platformBinariesDir auto-discovery', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-publish-dir-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const platformRoot = path.join(cwd, 'platform-binaries');
  const linuxBinary = path.join(platformRoot, 'linux-x64', 'lania-cli');
  await mkdir(path.dirname(linuxBinary), { recursive: true });
  await writeFile(linuxBinary, '#!/bin/sh\nexit 0\n', 'utf8');

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  await productPlugin.handle('product.pack', { cwd });
  const published = await productPlugin.handle('product.publish', {
    cwd,
    platformBinariesDir: platformRoot,
  });
  assert.ok(published);

  const publishRoot = path.join(cwd, '.lania', 'publish', 'product', 'npm-package');
  const publishReport = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-report.json'), 'utf8'),
  ) as {
    bundle: {
      platformTarballs: Array<{ packageName: string; platform: string; tarball: string; source: string }>;
      platformMatrix: Array<{ packageName: string; platform: string; status: string; source: string }>;
    };
  };
  const linuxTarball = publishReport.bundle.platformTarballs.find(
    (entry) => entry.packageName === '@lania-cli/cli-linux-x64',
  );
  assert.ok(linuxTarball);
  assert.equal(linuxTarball.platform, 'linux-x64');
  assert.equal(linuxTarball.source, 'request');
  const linuxMatrix = publishReport.bundle.platformMatrix.find(
    (entry) => entry.packageName === '@lania-cli/cli-linux-x64',
  );
  assert.equal(linuxMatrix?.status, 'ready');
  assert.equal(linuxMatrix?.source, 'request');
});

test('product.publish can execute publish-manifest steps through npm publish --dry-run', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-publish-exec-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  const fakeBinDir = path.join(cwd, 'fake-bin');
  const fakeNpm = path.join(fakeBinDir, 'npm');
  const fakeLog = path.join(cwd, 'fake-npm.log');
  await mkdir(fakeBinDir, { recursive: true });
  await writeFile(fakeLog, '', 'utf8');
  await writeFile(
    fakeNpm,
    `#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
const logFile = process.env.LANIA_FAKE_NPM_LOG;
if (args[0] === 'pack') {
  const pkg = JSON.parse(fs.readFileSync(path.join(process.cwd(), 'package.json'), 'utf8'));
  const filename = pkg.name.replace('@', '').replace('/', '-') + '-' + pkg.version + '.tgz';
  fs.writeFileSync(path.join(process.cwd(), filename), 'fake tgz');
  process.stdout.write(JSON.stringify([{ filename }]));
  process.exit(0);
}
if (args[0] === 'whoami') {
  process.stdout.write('fake-user\\n');
  process.exit(0);
}
if (args[0] === 'view') {
  process.exit(1);
}
if (args[0] === 'publish') {
  fs.appendFileSync(logFile, JSON.stringify({ cwd: process.cwd(), args }) + '\\n');
  process.exit(0);
}
process.stdout.write(JSON.stringify([{ filename: 'noop.tgz' }]));
process.exit(0);
`,
    'utf8',
  );
  await chmod(fakeNpm, 0o755);
  await writeFile(path.join(cwd, 'linux-binary'), '#!/bin/sh\nexit 0\n', 'utf8');

  const previousPath = process.env.PATH;
  const previousLog = process.env.LANIA_FAKE_NPM_LOG;
  process.env.PATH = `${fakeBinDir}${path.delimiter}${previousPath ?? ''}`;
  process.env.LANIA_FAKE_NPM_LOG = fakeLog;
  process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64 = path.join(cwd, 'linux-binary');
  t.after(() => {
    process.env.PATH = previousPath;
    if (previousLog === undefined) {
      delete process.env.LANIA_FAKE_NPM_LOG;
    } else {
      process.env.LANIA_FAKE_NPM_LOG = previousLog;
    }
    delete process.env.LANIA_PRODUCT_BINARY_SOURCE_LINUX_X64;
  });

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: {
        name: '@acme/demo',
        binaryName: 'acme',
        templatesDir: './product/templates'
      },
      schema: {
        entry: './product/lania.schemas.ts'
      }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  await productPlugin.handle('product.pack', { cwd });
  const published = await productPlugin.handle('product.publish', {
    cwd,
    execute: true,
    dryRun: true,
  });
  assert.ok(published);

  const publishRoot = path.join(cwd, '.lania', 'publish', 'product', 'npm-package');
  const publishManifest = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-manifest.json'), 'utf8'),
  ) as {
    execution: { executed: boolean; dryRun: boolean; completedSteps: string[] };
    steps: Array<{ id: string; packageName: string; command: { args: string[] } }>;
  };
  assert.equal(publishManifest.execution.executed, true);
  assert.equal(publishManifest.execution.dryRun, true);
  assert.deepEqual(
    publishManifest.execution.completedSteps,
    publishManifest.steps.map((entry) => entry.id),
  );

  const lines = (await readFile(fakeLog, 'utf8'))
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { args: string[] });
  const publishLines = lines.filter((entry) => entry.args[0] === 'publish');
  assert.equal(publishLines.length, publishManifest.steps.length);
  assert.ok(publishLines.every((entry) => entry.args.includes('--dry-run')));
  assert.deepEqual(
    publishLines.map((entry) => entry.args[1]),
    publishManifest.steps.map((entry) => entry.command.args[1]),
  );

  const publishReport = JSON.parse(
    await readFile(path.join(publishRoot, 'publish-report.json'), 'utf8'),
  ) as {
    experimental?: {
      registryPublish?: {
        execution?: { executed: boolean; dryRun: boolean; completedSteps: string[] };
      };
    };
  };
  assert.deepEqual(
    publishReport.experimental?.registryPublish?.execution,
    publishManifest.execution,
  );
});

test('scripts.publish can execute publish-manifest through npm publish --dry-run', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-publish-script-manifest-'));
  t.after(async () => rm(root, { recursive: true, force: true }));

  const fakeBinDir = path.join(root, 'fake-bin');
  const fakeNpm = path.join(fakeBinDir, 'npm');
  const fakeLog = path.join(root, 'fake-npm.log');
  const manifestRoot = path.join(root, 'manifest-root');
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-package'), { recursive: true });
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-linux-x64-package'), { recursive: true });
  await mkdir(fakeBinDir, { recursive: true });
  await writeFile(fakeLog, '', 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'cli-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(
    path.join(manifestRoot, 'bundle', 'cli-linux-x64-package', 'pkg.tgz'),
    'fake',
    'utf8',
  );
  await writeFile(
    fakeNpm,
    `#!/usr/bin/env node
import { appendFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === 'whoami') {
  process.stdout.write('fake-user\\n');
  process.exit(0);
}
if (args[0] === 'view') {
  process.exit(1);
}
appendFileSync(process.env.LANIA_FAKE_NPM_LOG, JSON.stringify({ cwd: process.cwd(), args: process.argv.slice(2) }) + '\\n');
process.exit(0);
`,
    'utf8',
  );
  await chmod(fakeNpm, 0o755);

  const manifestPath = path.join(manifestRoot, 'publish-manifest.json');
  await writeFile(
    manifestPath,
    JSON.stringify(
      {
        steps: [
          {
            id: 'publish-1',
            packageName: '@lania-cli/cli-linux-x64',
            command: {
              program: 'npm',
              args: [
                'publish',
                './bundle/cli-linux-x64-package/pkg.tgz',
                '--tag',
                'next',
                '--access',
                'public',
                '--registry',
                'https://registry.npmjs.org/',
              ],
            },
          },
          {
            id: 'publish-2',
            packageName: '@lania-cli/cli',
            command: {
              program: 'npm',
              args: [
                'publish',
                './bundle/cli-package/pkg.tgz',
                '--tag',
                'next',
                '--access',
                'public',
                '--registry',
                'https://registry.npmjs.org/',
              ],
            },
          },
        ],
      },
      null,
      2,
    ),
    'utf8',
  );

  const run = spawnSync(
    process.execPath,
    [
      path.join(repoRoot, 'scripts', 'publish.mjs'),
      '--manifest',
      manifestPath,
      '--dry-run',
      '--npm-bin',
      fakeNpm,
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        LANIA_FAKE_NPM_LOG: fakeLog,
      },
    },
  );
  assert.equal(run.status, 0, `stdout:\n${run.stdout}\nstderr:\n${run.stderr}`);

  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
    execution: { executed: boolean; dryRun: boolean; completedSteps: string[] };
    steps: Array<{ id: string; command: { args: string[] } }>;
  };
  assert.equal(manifest.execution.executed, true);
  assert.equal(manifest.execution.dryRun, true);
  assert.deepEqual(manifest.execution.completedSteps, manifest.steps.map((entry) => entry.id));

  const lines = (await readFile(fakeLog, 'utf8'))
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { args: string[] });
  const publishLines = lines.filter((entry) => entry.args[0] === 'publish');
  assert.equal(publishLines.length, 2);
  assert.ok(publishLines.every((entry) => entry.args.includes('--dry-run')));
});

test('product.publish requires --yes for non-dry-run execution', async (t) => {
  const cwd = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-product-publish-yes-'));
  t.after(async () => rm(cwd, { recursive: true, force: true }));

  await mkdir(path.join(cwd, 'product', 'templates', 'base'), { recursive: true });
  await writeFile(
    path.join(cwd, 'package.json'),
    JSON.stringify({ name: '@acme/demo', version: '1.2.3' }, null, 2),
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'lan.config.cjs'),
    `module.exports = {
      extensions: { dynamicCommands: true },
      product: { name: '@acme/demo', binaryName: 'acme', templatesDir: './product/templates' },
      schema: { entry: './product/lania.schemas.ts' }
    };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'lania.schemas.ts'),
    `export default { commands: [{ name: 'hello', workflow: 'hello' }], workflows: { hello: async () => undefined } };`,
    'utf8',
  );
  await writeFile(
    path.join(cwd, 'product', 'templates', 'base', 'template.json'),
    JSON.stringify({ id: 'base' }, null, 2),
    'utf8',
  );

  await productPlugin.handle('product.build', { cwd });
  await productPlugin.handle('product.pack', { cwd });
  await assert.rejects(
    () =>
      productPlugin.handle('product.publish', {
        cwd,
        execute: true,
      }),
    /--yes/,
  );
});

test('scripts.publish can resume from completed manifest steps', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-publish-script-resume-'));
  t.after(async () => rm(root, { recursive: true, force: true }));

  const fakeBinDir = path.join(root, 'fake-bin');
  const fakeNpm = path.join(fakeBinDir, 'npm');
  const fakeLog = path.join(root, 'fake-npm.log');
  const manifestRoot = path.join(root, 'manifest-root');
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-package'), { recursive: true });
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-linux-x64-package'), { recursive: true });
  await mkdir(fakeBinDir, { recursive: true });
  await writeFile(fakeLog, '', 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'cli-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(
    path.join(manifestRoot, 'bundle', 'cli-linux-x64-package', 'pkg.tgz'),
    'fake',
    'utf8',
  );
  await writeFile(
    fakeNpm,
    `#!/usr/bin/env node
import { appendFileSync } from 'node:fs';
appendFileSync(process.env.LANIA_FAKE_NPM_LOG, JSON.stringify({ cwd: process.cwd(), args: process.argv.slice(2) }) + '\\n');
process.exit(0);
`,
    'utf8',
  );
  await chmod(fakeNpm, 0o755);

  const manifestPath = path.join(manifestRoot, 'publish-manifest.json');
  await writeFile(
    manifestPath,
    JSON.stringify(
      {
        steps: [
          {
            id: 'publish-1',
            packageName: '@lania-cli/cli-linux-x64',
            command: {
              program: 'npm',
              args: ['publish', './bundle/cli-linux-x64-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
          {
            id: 'publish-2',
            packageName: '@lania-cli/cli',
            command: {
              program: 'npm',
              args: ['publish', './bundle/cli-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
        ],
        execution: {
          executed: false,
          dryRun: true,
          completedSteps: ['publish-1'],
        },
      },
      null,
      2,
    ),
    'utf8',
  );

  const run = spawnSync(
    process.execPath,
    [
      path.join(repoRoot, 'scripts', 'publish.mjs'),
      '--manifest',
      manifestPath,
      '--dry-run',
      '--resume',
      '--npm-bin',
      fakeNpm,
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        LANIA_FAKE_NPM_LOG: fakeLog,
      },
    },
  );
  assert.equal(run.status, 0, `stdout:\n${run.stdout}\nstderr:\n${run.stderr}`);

  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
    execution: { completedSteps: string[]; resumed: boolean };
  };
  assert.equal(manifest.execution.resumed, true);
  assert.deepEqual(manifest.execution.completedSteps, ['publish-1', 'publish-2']);

  const lines = (await readFile(fakeLog, 'utf8'))
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { args: string[] });
  const publishLines = lines.filter((entry) => entry.args[0] === 'publish');
  assert.equal(publishLines.length, 1);
  assert.equal(publishLines[0]?.args[1], './bundle/cli-package/pkg.tgz');
});

test('scripts.configure-npm-auth infers registry from manifest and writes tokenized npmrc', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-publish-auth-'));
  t.after(async () => rm(root, { recursive: true, force: true }));

  const manifestPath = path.join(root, 'publish-manifest.json');
  const outputPath = path.join(root, '.npmrc.publish');
  await writeFile(
    manifestPath,
    JSON.stringify(
      {
        steps: [
          {
            publishConfig: {
              registry: 'https://registry.example.com/npm/',
            },
            command: {
              args: [],
            },
          },
        ],
      },
      null,
      2,
    ),
    'utf8',
  );

  const run = spawnSync(
    process.execPath,
    [
      path.join(repoRoot, 'scripts', 'configure-npm-auth.mjs'),
      '--manifest',
      manifestPath,
      '--output',
      outputPath,
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        NODE_AUTH_TOKEN: 'test-token',
      },
    },
  );
  assert.equal(run.status, 0, `stdout:\n${run.stdout}\nstderr:\n${run.stderr}`);

  const npmrc = await readFile(outputPath, 'utf8');
  assert.match(npmrc, /registry=https:\/\/registry\.example\.com\/npm\//);
  assert.match(npmrc, /always-auth=true/);
  assert.match(npmrc, /\/\/registry\.example\.com\/npm\/:_authToken=test-token/);
});

test('scripts.publish retries transient publish failures and records attempts', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-publish-script-retry-'));
  t.after(async () => rm(root, { recursive: true, force: true }));

  const fakeBinDir = path.join(root, 'fake-bin');
  const fakeNpm = path.join(fakeBinDir, 'npm');
  const fakeLog = path.join(root, 'fake-npm.log');
  const stateFile = path.join(root, 'state.json');
  const manifestRoot = path.join(root, 'manifest-root');
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-package'), { recursive: true });
  await mkdir(path.join(manifestRoot, 'bundle', 'product-package'), { recursive: true });
  await mkdir(fakeBinDir, { recursive: true });
  await writeFile(fakeLog, '', 'utf8');
  await writeFile(stateFile, JSON.stringify({ publish1Failures: 0 }, null, 2), 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'cli-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'product-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(
    fakeNpm,
    `#!/usr/bin/env node
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === 'whoami') {
  process.stdout.write('fake-user\\n');
  process.exit(0);
}
if (args[0] === 'view') {
  process.exit(1);
}
if (args[0] === 'publish' && args[1] === './bundle/cli-package/pkg.tgz') {
  const state = JSON.parse(readFileSync(process.env.LANIA_FAKE_NPM_STATE, 'utf8'));
  if (state.publish1Failures === 0) {
    state.publish1Failures = 1;
    writeFileSync(process.env.LANIA_FAKE_NPM_STATE, JSON.stringify(state));
    process.stderr.write('ECONNRESET transient registry failure\\n');
    process.exit(1);
  }
}
appendFileSync(process.env.LANIA_FAKE_NPM_LOG, JSON.stringify({ args }) + '\\n');
process.exit(0);
`,
    'utf8',
  );
  await chmod(fakeNpm, 0o755);

  const manifestPath = path.join(manifestRoot, 'publish-manifest.json');
  await writeFile(
    manifestPath,
    JSON.stringify(
      {
        packages: [
          { name: '@lania-cli/cli', version: '1.0.0' },
          { name: '@acme/demo', version: '1.0.0' },
        ],
        steps: [
          {
            id: 'publish-1',
            packageName: '@lania-cli/cli',
            publishConfig: { registry: 'https://registry.npmjs.org/' },
            command: {
              program: 'npm',
              args: ['publish', './bundle/cli-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
          {
            id: 'publish-2',
            packageName: '@acme/demo',
            publishConfig: { registry: 'https://registry.npmjs.org/' },
            command: {
              program: 'npm',
              args: ['publish', './bundle/product-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
        ],
      },
      null,
      2,
    ),
    'utf8',
  );

  const run = spawnSync(
    process.execPath,
    [
      path.join(repoRoot, 'scripts', 'publish.mjs'),
      '--manifest',
      manifestPath,
      '--dry-run',
      '--npm-bin',
      fakeNpm,
      '--max-retries',
      '2',
      '--retry-delay-ms',
      '0',
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        LANIA_FAKE_NPM_LOG: fakeLog,
        LANIA_FAKE_NPM_STATE: stateFile,
      },
    },
  );
  assert.equal(run.status, 0, `stdout:\n${run.stdout}\nstderr:\n${run.stderr}`);

  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
    execution: {
      completedSteps: string[];
      retryPolicy: { maxRetries: number; retryDelayMs: number };
      attempts: Array<{
        stepId: string;
        attempt: number;
        status: string;
        retriable: boolean;
      }>;
    };
  };
  assert.deepEqual(manifest.execution.completedSteps, ['publish-1', 'publish-2']);
  assert.deepEqual(manifest.execution.retryPolicy, { maxRetries: 2, retryDelayMs: 0 });
  const publish1Attempts = manifest.execution.attempts.filter((entry) => entry.stepId === 'publish-1');
  assert.equal(publish1Attempts.length, 2);
  assert.equal(publish1Attempts[0]?.status, 'failed');
  assert.equal(publish1Attempts[0]?.retriable, true);
  assert.equal(publish1Attempts[1]?.status, 'succeeded');
});

test('scripts.publish can execute rollback commands after partial publish failure', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-publish-script-rollback-'));
  t.after(async () => rm(root, { recursive: true, force: true }));

  const fakeBinDir = path.join(root, 'fake-bin');
  const fakeNpm = path.join(fakeBinDir, 'npm');
  const fakeLog = path.join(root, 'fake-npm.log');
  const manifestRoot = path.join(root, 'manifest-root');
  await mkdir(path.join(manifestRoot, 'bundle', 'cli-package'), { recursive: true });
  await mkdir(path.join(manifestRoot, 'bundle', 'product-package'), { recursive: true });
  await mkdir(fakeBinDir, { recursive: true });
  await writeFile(fakeLog, '', 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'cli-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(path.join(manifestRoot, 'bundle', 'product-package', 'pkg.tgz'), 'fake', 'utf8');
  await writeFile(
    fakeNpm,
    `#!/usr/bin/env node
import { appendFileSync } from 'node:fs';
const args = process.argv.slice(2);
if (args[0] === 'whoami') {
  process.stdout.write('fake-user\\n');
  process.exit(0);
}
if (args[0] === 'view') {
  process.exit(1);
}
appendFileSync(process.env.LANIA_FAKE_NPM_LOG, JSON.stringify({ args }) + '\\n');
if (args[0] === 'publish' && args[1] === './bundle/product-package/pkg.tgz') {
  process.stderr.write('fatal publish failure\\n');
  process.exit(1);
}
process.exit(0);
`,
    'utf8',
  );
  await chmod(fakeNpm, 0o755);

  const manifestPath = path.join(manifestRoot, 'publish-manifest.json');
  await writeFile(
    manifestPath,
    JSON.stringify(
      {
        packages: [
          { name: '@lania-cli/cli', version: '1.0.0' },
          { name: '@acme/demo', version: '1.0.0' },
        ],
        steps: [
          {
            id: 'publish-1',
            packageName: '@lania-cli/cli',
            publishConfig: { registry: 'https://registry.npmjs.org/' },
            command: {
              program: 'npm',
              args: ['publish', './bundle/cli-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
          {
            id: 'publish-2',
            packageName: '@acme/demo',
            publishConfig: { registry: 'https://registry.npmjs.org/' },
            command: {
              program: 'npm',
              args: ['publish', './bundle/product-package/pkg.tgz', '--tag', 'next', '--access', 'public', '--registry', 'https://registry.npmjs.org/'],
            },
          },
        ],
      },
      null,
      2,
    ),
    'utf8',
  );

  const run = spawnSync(
    process.execPath,
    [
      path.join(repoRoot, 'scripts', 'publish.mjs'),
      '--manifest',
      manifestPath,
      '--yes',
      '--rollback-on-failure',
      '--npm-bin',
      fakeNpm,
    ],
    {
      cwd: repoRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        LANIA_FAKE_NPM_LOG: fakeLog,
      },
    },
  );
  assert.notEqual(run.status, 0);

  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
    execution: {
      failedStepId: string | null;
      rollbackPlan: {
        status: string;
        commands: Array<{ command: string[] }>;
      };
    };
  };
  assert.equal(manifest.execution.failedStepId, 'publish-2');
  assert.equal(manifest.execution.rollbackPlan.status, 'executed');
  assert.equal(manifest.execution.rollbackPlan.commands.length, 1);
  assert.deepEqual(manifest.execution.rollbackPlan.commands[0]?.command, [
    'unpublish',
    '@lania-cli/cli@1.0.0',
    '--registry',
    'https://registry.npmjs.org/',
  ]);

  const lines = (await readFile(fakeLog, 'utf8'))
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { args: string[] });
  assert.equal(lines.filter((entry) => entry.args[0] === 'publish').length, 2);
  assert.deepEqual(
    lines.find((entry) => entry.args[0] === 'unpublish')?.args,
    ['unpublish', '@lania-cli/cli@1.0.0', '--registry', 'https://registry.npmjs.org/'],
  );
});
