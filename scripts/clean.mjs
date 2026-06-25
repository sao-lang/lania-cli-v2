import console from 'node:console';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function discoverPlatformPackageDescriptors() {
  const cli = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  const optionalDependencies = cli.optionalDependencies ?? {};
  return Object.keys(optionalDependencies)
    .filter((packageName) => packageName.startsWith('@lania-cli/cli-'))
    .map((packageName) => {
      const packageDirName = packageName.replace('@lania-cli/', '');
      return {
        packageName,
        packageDirName,
      };
    })
    .sort((left, right) => left.packageName.localeCompare(right.packageName));
}

function collectPathsToRemove() {
  const targets = [
    'rust/target',
    'npm/cli/lib/node-bridge',
    'npm/cli/*.tgz',
    'ts/packages/node-bridge/dist',
    'ts/packages/templates/dist',
  ];
  for (const descriptor of discoverPlatformPackageDescriptors()) {
    targets.push(`npm/${descriptor.packageDirName}/bin/lania-cli`);
    targets.push(`npm/${descriptor.packageDirName}/*.tgz`);
  }
  return targets;
}

function removePath(targetPath) {
  if (!fs.existsSync(targetPath)) {
    return false;
  }
  fs.rmSync(targetPath, { recursive: true, force: true });
  return true;
}

function removeMatches(pattern) {
  const normalized = pattern.replaceAll('\\', '/');
  if (!normalized.includes('*')) {
    return removePath(path.join(repoRoot, normalized)) ? 1 : 0;
  }

  const lastSlash = normalized.lastIndexOf('/');
  const dir = normalized.slice(0, lastSlash);
  const namePattern = normalized.slice(lastSlash + 1);
  const suffix = namePattern.startsWith('*') ? namePattern.slice(1) : namePattern;
  const absoluteDir = path.join(repoRoot, dir);
  if (!fs.existsSync(absoluteDir)) {
    return 0;
  }

  let removed = 0;
  for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
    if (!entry.name.endsWith(suffix)) {
      continue;
    }
    const full = path.join(absoluteDir, entry.name);
    fs.rmSync(full, { recursive: true, force: true });
    removed += 1;
  }
  return removed;
}

function main() {
  let removed = 0;
  for (const target of collectPathsToRemove()) {
    removed += removeMatches(target);
  }
  console.log(`[clean] removed ${removed} path(s)`);
}

main();
