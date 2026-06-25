import fs from 'node:fs';
import https from 'node:https';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function readRustWorkspaceVersion() {
  const cargoToml = fs.readFileSync(path.join(repoRoot, 'rust/Cargo.toml'), 'utf8');
  const marker = '[workspace.package]';
  const idx = cargoToml.indexOf(marker);
  if (idx === -1) {
    throw new Error('rust/Cargo.toml missing [workspace.package]');
  }
  const tail = cargoToml.slice(idx);
  const match = tail.match(/\nversion\s*=\s*"([^"]+)"/);
  if (!match) {
    throw new Error('rust/Cargo.toml missing workspace.package version');
  }
  return match[1];
}

function discoverPlatformPackageDescriptors() {
  const cli = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  const optionalDependencies = cli.optionalDependencies ?? {};
  return Object.entries(optionalDependencies)
    .filter(([packageName]) => packageName.startsWith('@lania-cli/cli-'))
    .map(([packageName, version]) => {
      const packageDirName = packageName.replace('@lania-cli/', '');
      const packageRoot = path.join(repoRoot, 'npm', packageDirName);
      const packageJsonPath = path.join(packageRoot, 'package.json');
      const packageExists = fs.existsSync(packageJsonPath);
      const packageJson = packageExists ? readJson(packageJsonPath) : {};
      const os = Array.isArray(packageJson.os) ? packageJson.os[0] : null;
      const cpu = Array.isArray(packageJson.cpu) ? packageJson.cpu[0] : null;
      return {
        packageName,
        packageDirName,
        packageRoot,
        version: packageJson.version ?? version,
        platform:
          typeof os === 'string' && typeof cpu === 'string'
            ? `${os}-${cpu}`
            : packageDirName.replace(/^cli-/, ''),
      };
    })
    .sort((left, right) => left.packageName.localeCompare(right.packageName));
}

function downloadFile(url, destination, headers = {}) {
  ensureDir(path.dirname(destination));
  return new Promise((resolve, reject) => {
    const request = https.get(url, { headers }, (response) => {
      if (
        response.statusCode &&
        response.statusCode >= 300 &&
        response.statusCode < 400 &&
        response.headers.location
      ) {
        response.resume();
        downloadFile(response.headers.location, destination, headers).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`GET ${url} failed with status ${response.statusCode}`));
        return;
      }
      const file = fs.createWriteStream(destination, { mode: 0o755 });
      response.pipe(file);
      file.on('finish', () => file.close((error) => (error ? reject(error) : resolve())));
      file.on('error', reject);
    });
    request.on('error', reject);
  });
}

function parsePlatform(platform) {
  const [os, arch] = String(platform).split('-', 2);
  return { os: os ?? platform, arch: arch ?? '' };
}

function splitTemplates(input, fallback) {
  const raw = typeof input === 'string' ? input.trim() : '';
  const source = raw.length > 0 ? raw : fallback;
  return source
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

function renderTemplate(template, vars) {
  let value = template;
  for (const [key, val] of Object.entries(vars)) {
    value = value.replaceAll(`{${key}}`, String(val));
  }
  return value;
}

function isArchiveFile(name) {
  return name.endsWith('.zip') || name.endsWith('.tar.gz') || name.endsWith('.tgz');
}

function listFilesRecursive(rootDir) {
  const result = [];
  const stack = [rootDir];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!current) continue;
    let entries = [];
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile()) {
        result.push(full);
      }
    }
  }
  return result;
}

function tryExtractArchive(archivePath, extractDir) {
  ensureDir(extractDir);
  if (archivePath.endsWith('.zip')) {
    const r = spawnSync('unzip', ['-o', archivePath, '-d', extractDir], {
      cwd: repoRoot,
      encoding: 'utf8',
    });
    if (r.status !== 0) {
      throw new Error(`unzip failed: ${r.stderr || r.stdout || 'unknown error'}`);
    }
    return;
  }
  const r = spawnSync('tar', ['-xzf', archivePath, '-C', extractDir], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  if (r.status !== 0) {
    throw new Error(`tar failed: ${r.stderr || r.stdout || 'unknown error'}`);
  }
}

function findExtractedBinary(extractDir) {
  const files = listFilesRecursive(extractDir);
  const matches = files.filter((file) => path.basename(file) === 'lania-cli');
  if (matches.length > 0) return matches[0];
  const alt = files.filter((file) => path.basename(file).startsWith('lania-cli'));
  return alt[0] ?? null;
}

function resolveDownloadConfig(version) {
  const repo = process.env.LANIA_CLI_BINARY_RELEASE_REPO?.trim();
  if (!repo) {
    return null;
  }
  const tagTemplate = process.env.LANIA_CLI_BINARY_RELEASE_TAG_TEMPLATE?.trim() || 'v{version}';
  const assetTemplates = splitTemplates(
    process.env.LANIA_CLI_BINARY_RELEASE_ASSET_TEMPLATE,
    'lania-cli-{platform},lania-cli-{os}-{arch},lania-cli_{platform}',
  );
  const outputDir =
    process.env.LANIA_CLI_PLATFORM_BINARIES_DIR?.trim() ||
    path.join(repoRoot, '.lania', 'platform-binaries');
  const token = process.env.GITHUB_TOKEN?.trim() || process.env.LANIA_GITHUB_TOKEN?.trim() || null;
  return {
    repo,
    tag: tagTemplate.replaceAll('{version}', version),
    assetTemplates,
    outputDir,
    token,
  };
}

export async function fetchPlatformBinaries() {
  const version = readRustWorkspaceVersion();
  const config = resolveDownloadConfig(version);
  if (!config) {
    return { enabled: false, outputDir: null, downloaded: [] };
  }
  const descriptors = discoverPlatformPackageDescriptors();
  const headers = {
    'User-Agent': 'lania-cli-fetch-platform-binaries',
    Accept: 'application/octet-stream',
    ...(config.token ? { Authorization: `Bearer ${config.token}` } : {}),
  };
  const downloaded = [];
  for (const descriptor of descriptors) {
    const destination = path.join(config.outputDir, descriptor.platform, 'lania-cli');
    const { os, arch } = parsePlatform(descriptor.platform);
    const vars = {
      version,
      platform: descriptor.platform,
      os,
      arch,
    };
    try {
      let resolved = null;
      let lastError = null;
      for (const template of config.assetTemplates) {
        const assetName = renderTemplate(template, vars);
        const url = `https://github.com/${config.repo}/releases/download/${config.tag}/${assetName}`;
        const staged = isArchiveFile(assetName)
          ? path.join(config.outputDir, descriptor.platform, `.download/${assetName}`)
          : destination;
        try {
          await downloadFile(url, staged, headers);
          if (staged !== destination) {
            const extractDir = path.join(config.outputDir, descriptor.platform, '.download/extract');
            tryExtractArchive(staged, extractDir);
            const extracted = findExtractedBinary(extractDir);
            if (!extracted) {
              throw new Error(`archive did not contain lania-cli under ${extractDir}`);
            }
            ensureDir(path.dirname(destination));
            fs.copyFileSync(extracted, destination);
          }
          fs.chmodSync(destination, 0o755);
          resolved = { url, template, assetName };
          break;
        } catch (error) {
          lastError = error;
        }
      }
      if (!resolved) {
        throw lastError ?? new Error('no asset template candidates succeeded');
      }
      downloaded.push({ platform: descriptor.platform, path: destination, url: resolved.url });
      console.log(
        `[fetch-platform-binaries] downloaded ${descriptor.platform} from ${resolved.url} (${resolved.template})`,
      );
    } catch (error) {
      console.warn(
        `[fetch-platform-binaries] skip ${descriptor.platform}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
  }
  return {
    enabled: true,
    outputDir: config.outputDir,
    downloaded,
  };
}

if (process.argv[1] && process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = await fetchPlatformBinaries();
  if (result.enabled) {
    console.log(
      `[fetch-platform-binaries] downloaded ${result.downloaded.length} binaries into ${result.outputDir}`,
    );
  } else {
    console.log('[fetch-platform-binaries] skipped: LANIA_CLI_BINARY_RELEASE_REPO not configured');
  }
}
