// 交互式发布脚本 — 基于 @clack/prompts
import { intro, outro, log, select, multiselect, text, confirm, isCancel } from '@clack/prompts';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

// ── 工具 ──

function read(filePath) { return fs.readFileSync(filePath, 'utf8'); }
function write(filePath, content) { fs.writeFileSync(filePath, content); }
function readJson(filePath) { return JSON.parse(read(filePath)); }
function writeJson(filePath, value) { write(filePath, `${JSON.stringify(value, null, 2)}\n`); }

function cancel(msg) { log.error(msg); process.exit(0); }

// ── 包发现 ──

function discoverAllPackages() {
  const cliPkg = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  const platformPkgs = Object.keys(cliPkg.optionalDependencies ?? {})
    .filter((n) => n.startsWith('@lania-cli/cli-'))
    .map((n) => ({
      name: n,
      packageJsonPath: path.join(repoRoot, 'npm', n.replace('@lania-cli/', ''), 'package.json'),
    }))
    .filter((p) => fs.existsSync(p.packageJsonPath));

  const all = [
    { name: '@lania-cli/cli',             packageJsonPath: path.join(repoRoot, 'npm/cli/package.json') },
    { name: '@lania-cli/node-bridge',     packageJsonPath: path.join(repoRoot, 'ts/packages/node-bridge/package.json') },
    { name: '@lania-cli/templates',       packageJsonPath: path.join(repoRoot, 'ts/packages/templates/package.json') },
    { name: 'lania',                      packageJsonPath: path.join(repoRoot, 'ts/packages/lania/package.json') },
    ...platformPkgs,
  ];
  return all.map((p) => ({ ...p, version: readJson(p.packageJsonPath).version }));
}

// ── 文件操作 ──

function bumpJsonVersion(filePath, version) {
  const json = readJson(filePath);
  json.version = version;
  writeJson(filePath, json);
}

function bumpRustWorkspaceVersion(version) {
  const cargoTomlPath = path.join(repoRoot, 'rust/Cargo.toml');
  const input = read(cargoTomlPath);
  const marker = '[workspace.package]';
  const idx = input.indexOf(marker);
  if (idx === -1) throw new Error('rust/Cargo.toml missing [workspace.package]');
  const before = input.slice(0, idx);
  const after = input.slice(idx);
  const replaced = after.replace(/\nversion\s*=\s*"[^"]+"\n/, `\nversion = "${version}"\n`);
  if (replaced === after) throw new Error('Failed to replace workspace.package version');
  write(cargoTomlPath, before + replaced);
}

function bumpCliOptionalDeps(version) {
  const cliPkg = readJson(path.join(repoRoot, 'npm/cli/package.json'));
  for (const [key] of Object.entries(cliPkg.optionalDependencies ?? {})) {
    if (key.startsWith('@lania-cli/cli-')) cliPkg.optionalDependencies[key] = version;
  }
  writeJson(path.join(repoRoot, 'npm/cli/package.json'), cliPkg);
}

function writeChangelog(version, entries) {
  if (entries.length === 0) return;
  const date = new Date().toISOString().split('T')[0];
  const content = entries.map((e) => `- ${e}`).join('\n');
  const section = `## ${version} (${date})\n\n${content}\n\n`;
  const changelogPath = path.join(repoRoot, 'CHANGELOG.md');
  const existing = fs.existsSync(changelogPath) ? read(changelogPath) : '';
  write(changelogPath, `# Changelog\n\n${section}${existing.startsWith('# Changelog') ? existing.slice('# Changelog'.length) : existing}`);
  log.success('CHANGELOG.md 已更新');
}

// ── 主流程 ──

async function main() {
  intro('Lania CLI Release Tool');

  // 1. 选择包
  const packages = discoverAllPackages();
  const selected = await multiselect({
    message: '选择要发布的包（空格选中/取消，Enter 确认）',
    options: packages.map((p) => ({
      value: p.name,
      label: `${p.name.padEnd(38)} v${p.version}`,
    })),
    required: true,
  });
  if (isCancel(selected)) cancel('已取消');

  // 2. 选择版本类型
  const currentVersion = readJson(packages.find((p) => p.name === selected[0]).packageJsonPath).version;
  const bumpType = await select({
    message: `选择版本更新类型（当前 ${currentVersion}）`,
    options: [
      { value: 'patch', label: 'patch', hint: `${currentVersion} → ${bumpVersion(currentVersion, 'patch')} — 修复补丁` },
      { value: 'minor', label: 'minor', hint: `${currentVersion} → ${bumpVersion(currentVersion, 'minor')} — 新功能` },
      { value: 'major', label: 'major', hint: `${currentVersion} → ${bumpVersion(currentVersion, 'major')} — 不兼容变更` },
      { value: 'custom', label: 'custom', hint: '手动输入版本号' },
    ],
  });
  if (isCancel(bumpType)) cancel('已取消');

  const newVersion = bumpType === 'custom'
    ? await text({ message: '输入版本号', validate: (v) => /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(v) ? undefined : '无效版本号格式' })
    : bumpVersion(currentVersion, bumpType);
  if (isCancel(newVersion)) cancel('已取消');

  // 3. 输入 changelog
  const changelogRaw = await text({
    message: '输入 Changelog 条目（多行用 | 分隔，如: feat: xxx | fix: yyy）',
    placeholder: 'feat: 添加产品发布能力 | fix: 修复 Windows 打包问题',
  });
  if (isCancel(changelogRaw)) cancel('已取消');
  const changelogEntries = changelogRaw.trim() ? changelogRaw.split('|').map((s) => s.trim()).filter(Boolean) : [];

  // 4. 预览确认
  log.info(`版本: ${currentVersion} → ${newVersion}`);
  log.info(`包数量: ${selected.length}`);
  selected.forEach((name) => log.step(`  ${name}`));
  if (changelogEntries.length > 0) {
    log.info('Changelog:');
    changelogEntries.forEach((e) => log.step(`  - ${e}`));
  }

  const confirmed = await confirm({ message: '确认发布？', initialValue: false });
  if (isCancel(confirmed) || !confirmed) cancel('已取消');

  // 5. 执行
  bumpRustWorkspaceVersion(newVersion);
  log.step('✓ rust/Cargo.toml');

  const updated = new Set();
  for (const name of selected) {
    const pkg = packages.find((p) => p.name === name);
    if (!pkg || updated.has(pkg.packageJsonPath)) continue;
    updated.add(pkg.packageJsonPath);
    bumpJsonVersion(pkg.packageJsonPath, newVersion);
    log.step(`✓ ${name}`);
  }
  bumpCliOptionalDeps(newVersion);
  log.step('✓ @lania-cli/cli optionalDependencies');
  writeChangelog(newVersion, changelogEntries);

  outro(`✅ 版本已更新至 ${newVersion}

下一步:
  git add -A
  git commit -m "chore: release v${newVersion}"
  git tag v${newVersion}
  git push && git push --tags
  pnpm pack
  pnpm publish:all`);
}

function bumpVersion(current, type) {
  const parts = current.split('.').map(Number);
  if (type === 'major') return `${parts[0] + 1}.0.0`;
  if (type === 'minor') return `${parts[0]}.${parts[1] + 1}.0`;
  return `${parts[0]}.${parts[1]}.${parts[2] + 1}`;
}

main().catch((err) => { console.error(err); process.exit(1); });
