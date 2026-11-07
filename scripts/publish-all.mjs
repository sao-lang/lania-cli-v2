#!/usr/bin/env node
/**
 * 一键发布所有 npm 产物（文档 §6.2 / §6.4）。
 *
 * - 包清单来自 `lania.package.json`（manifest 驱动），不再硬编码 4 行；
 * - 发布顺序按**拓扑排序**：平台包 → 命令包 / 设施包 → core → 总包；
 *   依赖来自同 workspace 包的 npm dependencies / optionalDependencies。
 *
 * 用法：
 *   node scripts/publish-all.mjs --dry-run   # 只打印发布序列
 *   node scripts/publish-all.mjs             # 真正发布
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { discoverPackages } from './lib/discover-packages.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dryRun = process.argv.includes('--dry-run');

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

/**
 * 收集全部 npm 产物：主包 + 平台包。
 * 每项记录 { name, dir, deps }，deps 是它依赖的同 workspace 包名。
 */
function collectPublishTargets() {
  const targets = [];
  for (const entry of discoverPackages()) {
    const npm = entry.manifest.npm;
    if (!npm?.name) continue;
    const npmDir = path.join(entry.dir, 'npm', npm.name.replace('@lania-cli-v2/', ''));
    const packageJsonPath = path.join(npmDir, 'package.json');
    if (!fs.existsSync(packageJsonPath)) continue;
    const pkg = readJson(packageJsonPath);
    const deps = [
      ...Object.keys(pkg.dependencies ?? {}),
      ...Object.keys(pkg.optionalDependencies ?? {}),
    ].filter((name) => name.startsWith('@lania-cli-v2/'));
    targets.push({
      name: npm.name,
      dir: npmDir,
      family: entry.manifest.versionFamily,
      kind: entry.manifest.kind,
      deps,
    });
    for (const platformPackage of npm.platformPackages ?? []) {
      const platformDir = path.join(
        path.dirname(npmDir),
        platformPackage.replace('@lania-cli-v2/', ''),
      );
      const platformJsonPath = path.join(platformDir, 'package.json');
      if (!fs.existsSync(platformJsonPath)) continue;
      const platformPkg = readJson(platformJsonPath);
      targets.push({
        name: platformPackage,
        dir: platformDir,
        family: entry.manifest.versionFamily,
        kind: 'platform',
        deps: [
          ...Object.keys(platformPkg.dependencies ?? {}),
          ...Object.keys(platformPkg.optionalDependencies ?? {}),
        ].filter((name) => name.startsWith('@lania-cli-v2/')),
      });
    }
  }
  return targets;
}

/**
 * 拓扑排序：被依赖者先发。
 * 同层内保持 manifest 枚举顺序，保证结果稳定可复现。
 */
function topoSort(targets) {
  const byName = new Map(targets.map((target) => [target.name, target]));
  const visited = new Set();
  const visiting = new Set();
  const sorted = [];

  const visit = (target) => {
    if (visited.has(target.name)) return;
    if (visiting.has(target.name)) {
      // 环：按先到先发处理（workspace 内允许互依，如 cli ↔ core）
      return;
    }
    visiting.add(target.name);
    for (const dep of target.deps) {
      const depTarget = byName.get(dep);
      if (depTarget) visit(depTarget);
    }
    visiting.delete(target.name);
    visited.add(target.name);
    sorted.push(target);
  };

  for (const target of targets) visit(target);
  return sorted;
}

function publishPackage(target) {
  console.log(`\n[发布] ${target.name} ...`);
  if (dryRun) {
    console.log(`  （dry-run）目录：${path.relative(repoRoot, target.dir)}`);
    return;
  }
  const result = spawnSync('npm', ['publish', '--access', 'public'], {
    cwd: target.dir,
    stdio: 'inherit',
    env: process.env,
  });
  if (result.status !== 0) {
    console.error(`[失败] ${target.name}`);
    process.exit(result.status ?? 1);
  }
  console.log(`[完成] ${target.name}`);
}

const targets = topoSort(collectPublishTargets());
console.log(`[发布序列] 共 ${targets.length} 个包${dryRun ? '（dry-run）' : ''}：`);
for (const target of targets) {
  console.log(`  ${target.name}（${target.family}${target.kind === 'platform' ? ' / 平台包' : ''}）`);
}
for (const target of targets) publishPackage(target);
console.log('\n✅ 全部发布完成');
