import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function syncRustWorkspaceVersion(version) {
  const cargoTomlPath = path.join(repoRoot, 'rust/Cargo.toml');
  const input = fs.readFileSync(cargoTomlPath, 'utf8');
  const marker = '[workspace.package]';
  const idx = input.indexOf(marker);
  if (idx === -1) {
    throw new Error('rust/Cargo.toml missing [workspace.package]');
  }
  const before = input.slice(0, idx);
  const after = input.slice(idx);
  const replaced = after.replace(/\nversion\s*=\s*"[^"]+"\n/, `\nversion = "${version}"\n`);
  if (replaced === after) {
    throw new Error('Failed to update rust workspace version');
  }
  fs.writeFileSync(cargoTomlPath, before + replaced);
}

function main() {
  const cliPkg = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  syncRustWorkspaceVersion(cliPkg.version);
  console.log(`Synced Rust workspace version to ${cliPkg.version}`);
}

main();
