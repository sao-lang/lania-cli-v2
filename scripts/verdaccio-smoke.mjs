import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { clearTimeout, setTimeout } from 'node:timers';
import { URL } from 'node:url';

const fetch = globalThis.fetch.bind(globalThis);
const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const registryHost = '127.0.0.1';
const registryPort = 4873;
const registry = `http://${registryHost}:${registryPort}/`;
const scope = '@lania-smoke';

async function main() {
  const tempRoot = await mkdtemp(path.join(tmpdir(), 'lania-verdaccio-smoke-'));
  const verdaccio = await startVerdaccio(tempRoot);
  try {
    await waitForRegistry(`${registry}-/ping`, verdaccio);
    const token = await ensureVerdaccioToken(tempRoot);
    const npmrcPath = path.join(tempRoot, '.npmrc');
    configureNpmAuth(npmrcPath, token);

    const version = `0.0.0-smoke.${Date.now()}`;
    const cliPackageName = `${scope}/cli-${Date.now()}`;
    const productPackageName = `${scope}/product-${Date.now()}`;
    const packagesRoot = path.join(tempRoot, 'packages');
    const cliTarball = await createPackageTarball(packagesRoot, cliPackageName, version);
    const productTarball = await createPackageTarball(packagesRoot, productPackageName, version, {
      dependencies: {
        [cliPackageName]: version,
      },
    });
    const cliTarballRef = toPortableRelativePath(tempRoot, cliTarball);
    const productTarballRef = toPortableRelativePath(tempRoot, productTarball);
    const manifestPath = path.join(tempRoot, 'publish-manifest.json');
    await writeFile(
      manifestPath,
      JSON.stringify(
        {
          kind: 'product_publish_manifest',
          mode: 'registry_plan',
          outputRoot: tempRoot,
          packageName: productPackageName,
          packageVersion: version,
          binaryName: 'lania-smoke',
          distTag: 'latest',
          channel: 'latest',
          productTarball: productTarballRef,
          bundleRoot: null,
          packages: [
            {
              role: 'official_cli',
              name: cliPackageName,
              version,
              tarball: cliTarballRef,
              distTag: 'latest',
              channel: 'latest',
              publishStrategy: 'npm_tarball_publish',
            },
            {
              role: 'product',
              name: productPackageName,
              version,
              tarball: productTarballRef,
              distTag: 'latest',
              channel: 'latest',
              publishStrategy: 'npm_tarball_publish',
            },
          ],
          platformMatrix: [],
          publishOrder: [cliPackageName, productPackageName],
          dependencyLinks: [
            {
              from: productPackageName,
              to: cliPackageName,
              type: 'dependency',
              field: 'dependencies',
            },
          ],
          steps: [
            createPublishStep('publish-cli', cliPackageName, cliTarballRef, []),
            createPublishStep('publish-product', productPackageName, productTarballRef, [
              cliPackageName,
            ]),
          ],
          checks: {
            hasTarball: true,
          },
        },
        null,
        2,
      ) + '\n',
      'utf8',
    );

    runNode(
      [
        path.join(repoRoot, 'scripts', 'publish.mjs'),
        '--manifest',
        manifestPath,
        '--yes',
        '--max-retries',
        '2',
      ],
      {
        cwd: tempRoot,
        env: {
          ...process.env,
          NPM_CONFIG_USERCONFIG: npmrcPath,
        },
      },
    );

    const cliVersion = npmViewVersion(cliPackageName, npmrcPath);
    const productVersion = npmViewVersion(productPackageName, npmrcPath);
    if (cliVersion !== version || productVersion !== version) {
      throw new Error(
        `Verdaccio verification failed. Expected ${version}, got ${cliPackageName}@${cliVersion} and ${productPackageName}@${productVersion}`,
      );
    }
    process.stdout.write(
      JSON.stringify(
        {
          ok: true,
          registry,
          packages: [cliPackageName, productPackageName],
          version,
          manifestPath,
        },
        null,
        2,
      ) + '\n',
    );
  } finally {
    await stopVerdaccio(verdaccio);
    await rm(tempRoot, { recursive: true, force: true });
  }
}

function createPublishStep(id, packageName, tarball, dependsOn) {
  return {
    id,
    packageName,
    role: packageName.includes('/product-') ? 'product' : 'official_cli',
    tarball,
    distTag: 'latest',
    dependsOn,
    publishConfig: {
      registry,
      access: 'public',
      otpRequired: 'unknown',
      provenance: false,
      dryRun: false,
    },
    command: {
      program: 'npm',
      args: ['publish', tarball, '--tag', 'latest', '--access', 'public', '--registry', registry],
    },
  };
}

async function startVerdaccio(tempRoot) {
  const configPath = path.join(tempRoot, 'verdaccio.yaml');
  const logPath = path.join(tempRoot, 'verdaccio.log');
  await writeFile(
    configPath,
    [
      `storage: ${toYamlPath(path.join(tempRoot, 'storage'))}`,
      'auth:',
      '  htpasswd:',
      `    file: ${toYamlPath(path.join(tempRoot, 'htpasswd'))}`,
      'uplinks: {}',
      'packages:',
      "  '@*/*':",
      '    access: $all',
      '    publish: $authenticated',
      "  '**':",
      '    access: $all',
      '    publish: $authenticated',
      'server:',
      `  listen: ${registryHost}:${registryPort}`,
      'logs:',
      '  - { type: stdout, format: pretty, level: error }',
    ].join('\n') + '\n',
    'utf8',
  );
  const logStream = fs.createWriteStream(logPath, { flags: 'a' });
  const child = spawn(
    'npm',
    [
      'exec',
      '--yes',
      'verdaccio@6',
      '--',
      '--config',
      configPath,
      '--listen',
      `${registryHost}:${registryPort}`,
    ],
    {
      cwd: repoRoot,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );
  child.stdout.pipe(logStream);
  child.stderr.pipe(logStream);
  return {
    child,
    logPath,
  };
}

function toYamlPath(filePath) {
  return `"${filePath.replaceAll('\\', '/')}"`;
}

function toPortableRelativePath(baseDir, targetPath) {
  const relativePath = path.relative(baseDir, targetPath).replaceAll(path.sep, '/');
  return relativePath.startsWith('.') ? relativePath : `./${relativePath}`;
}

async function waitForRegistry(healthUrl, verdaccio) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (verdaccio.child.exitCode !== null) {
      const logs = await readFile(verdaccio.logPath, 'utf8').catch(() => '');
      throw new Error(`Verdaccio exited early with code ${verdaccio.child.exitCode}\n${logs}`);
    }
    try {
      const response = await fetch(healthUrl, { method: 'GET' });
      if (response.ok) {
        return;
      }
    } catch {
      // Ignore transient connection failures while Verdaccio is starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  const logs = await readFile(verdaccio.logPath, 'utf8').catch(() => '');
  throw new Error(`Timed out waiting for Verdaccio at ${healthUrl}\n${logs}`);
}

async function ensureVerdaccioToken(tempRoot) {
  const username = 'lania-smoke';
  const password = 'lania-smoke-pass';
  const email = 'lania-smoke@example.com';
  const createResponse = await fetch(
    `${registry}-/user/org.couchdb.user:${encodeURIComponent(username)}`,
    {
      method: 'PUT',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        name: username,
        password,
        email,
        type: 'user',
        roles: [],
        date: new Date().toISOString(),
      }),
    },
  );
  if (createResponse.ok) {
    const payload = await createResponse.json();
    if (typeof payload?.token === 'string' && payload.token.length > 0) {
      return payload.token;
    }
  }

  const userconfig = path.join(tempRoot, '.npmrc.login');
  const login = spawnSync(
    'npm',
    ['adduser', '--registry', registry, '--auth-type', 'legacy', '--userconfig', userconfig],
    {
      cwd: tempRoot,
      encoding: 'utf8',
      input: `${username}\n${password}\n${email}\n`,
      env: process.env,
    },
  );
  if (login.status !== 0) {
    throw new Error(
      `Unable to authenticate against Verdaccio.\nstdout:\n${login.stdout}\nstderr:\n${login.stderr}`,
    );
  }
  const npmrc = await readFile(userconfig, 'utf8');
  const tokenLine = npmrc
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.startsWith('//') && line.includes(':_authToken='));
  if (!tokenLine) {
    throw new Error(`Verdaccio login succeeded but token was not written to ${userconfig}`);
  }
  return tokenLine.split(':_authToken=').slice(1).join(':_authToken=').trim();
}

function configureNpmAuth(output, token) {
  runNode(
    [
      path.join(repoRoot, 'scripts', 'configure-npm-auth.mjs'),
      '--registry',
      registry,
      '--scope',
      scope,
      '--output',
      output,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        LANIA_NPM_TOKEN: token,
      },
    },
  );
}

async function createPackageTarball(root, packageName, version, extraPackageJson = {}) {
  await mkdir(root, { recursive: true });
  const packageDir = path.join(root, packageName.replace('@', '').replace('/', '__'));
  await mkdir(packageDir, { recursive: true });
  await writeFile(
    path.join(packageDir, 'package.json'),
    JSON.stringify(
      {
        name: packageName,
        version,
        type: 'module',
        main: './index.js',
        files: ['index.js'],
        ...extraPackageJson,
      },
      null,
      2,
    ) + '\n',
    'utf8',
  );
  await writeFile(path.join(packageDir, 'index.js'), 'export default true;\n', 'utf8');
  const packed = spawnSync('npm', ['pack', '--json'], {
    cwd: packageDir,
    encoding: 'utf8',
    env: process.env,
  });
  if (packed.status !== 0) {
    throw new Error(
      `npm pack failed for ${packageName}\nstdout:\n${packed.stdout}\nstderr:\n${packed.stderr}`,
    );
  }
  const parsed = JSON.parse(packed.stdout);
  return path.join(packageDir, parsed[0].filename);
}

function npmViewVersion(packageName, npmrcPath) {
  const result = spawnSync('npm', ['view', packageName, 'version', '--registry', registry], {
    cwd: repoRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      NPM_CONFIG_USERCONFIG: npmrcPath,
    },
  });
  if (result.status !== 0) {
    throw new Error(
      `npm view failed for ${packageName}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
  return result.stdout.trim();
}

function runNode(args, options) {
  const result = spawnSync(process.execPath, args, {
    cwd: options.cwd,
    encoding: 'utf8',
    env: options.env,
  });
  if (result.status !== 0) {
    throw new Error(
      `Command failed: node ${args.join(' ')}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
  return result;
}

async function stopVerdaccio(verdaccio) {
  if (verdaccio.child.exitCode !== null) {
    return;
  }
  verdaccio.child.kill('SIGTERM');
  await new Promise((resolve) => {
    const timer = setTimeout(() => {
      if (verdaccio.child.exitCode === null) {
        verdaccio.child.kill('SIGKILL');
      }
      resolve(undefined);
    }, 5_000);
    verdaccio.child.once('exit', () => {
      clearTimeout(timer);
      resolve(undefined);
    });
  });
}

await main();
