import console from 'node:console';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function die(message) {
  console.error(message);
  process.exit(1);
}

function read(filePath) {
  return fs.readFileSync(filePath, 'utf8');
}

function write(filePath, content) {
  fs.writeFileSync(filePath, content);
}

function readJson(filePath) {
  return JSON.parse(read(filePath));
}

function writeJson(filePath, value) {
  write(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function bumpRustWorkspaceVersion(version) {
  const cargoTomlPath = path.join(repoRoot, 'rust/Cargo.toml');
  const input = read(cargoTomlPath);
  const marker = '[workspace.package]';
  const idx = input.indexOf(marker);
  if (idx === -1) {
    die('rust/Cargo.toml missing [workspace.package]');
  }
  const before = input.slice(0, idx);
  const after = input.slice(idx);
  const replaced = after.replace(/\nversion\s*=\s*"[^"]+"\n/, `\nversion = "${version}"\n`);
  if (replaced === after) {
    die('rust/Cargo.toml: failed to replace workspace.package version');
  }
  write(cargoTomlPath, before + replaced);
}

function bumpJsonVersion(filePath, version) {
  const json = readJson(filePath);
  json.version = version;
  writeJson(filePath, json);
}

function discoverPlatformPackageDescriptors() {
  const cliPkgPath = path.join(repoRoot, 'npm/cli/package.json');
  const cli = readJson(cliPkgPath);
  cli.optionalDependencies ??= {};
  if (typeof cli.optionalDependencies !== 'object' || cli.optionalDependencies === null) {
    die('npm/cli/package.json: optionalDependencies must be an object');
  }
  return Object.keys(cli.optionalDependencies)
    .filter((packageName) => packageName.startsWith('@lania-cli/cli-'))
    .map((packageName) => {
      const packageDirName = packageName.replace('@lania-cli/', '');
      return {
        packageName,
        packageRoot: path.join(repoRoot, 'npm', packageDirName),
        packageJsonPath: path.join(repoRoot, 'npm', packageDirName, 'package.json'),
      };
    })
    .sort((left, right) => left.packageName.localeCompare(right.packageName));
}

function bumpCliOptionalDeps(version) {
  const cliPkgPath = path.join(repoRoot, 'npm/cli/package.json');
  const cli = readJson(cliPkgPath);
  cli.optionalDependencies ??= {};
  if (typeof cli.optionalDependencies !== 'object' || cli.optionalDependencies === null) {
    die('npm/cli/package.json: optionalDependencies must be an object');
  }
  for (const descriptor of discoverPlatformPackageDescriptors()) {
    cli.optionalDependencies[descriptor.packageName] = version;
  }
  writeJson(cliPkgPath, cli);
}

function main() {
  const version = process.argv[2];
  if (!version) {
    die('Usage: npm run version:bump -- <version>  (example: npm run version:bump -- 0.1.1)');
  }
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
    die(`Invalid version: ${version}`);
  }

  bumpRustWorkspaceVersion(version);
  bumpJsonVersion(path.join(repoRoot, 'ts/packages/node-bridge/package.json'), version);
  bumpJsonVersion(path.join(repoRoot, 'ts/packages/templates/package.json'), version);
  bumpJsonVersion(path.join(repoRoot, 'npm/cli/package.json'), version);
  for (const descriptor of discoverPlatformPackageDescriptors()) {
    if (!fs.existsSync(descriptor.packageJsonPath)) {
      console.warn(`[version:bump] skip ${descriptor.packageName}: package.json missing`);
      continue;
    }
    bumpJsonVersion(descriptor.packageJsonPath, version);
  }
  bumpCliOptionalDeps(version);

  console.log(`Bumped versions to ${version}`);
}

main();
