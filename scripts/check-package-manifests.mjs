#!/usr/bin/env node
/**
 * 包清单校验（CI 用）：node scripts/check-package-manifests.mjs
 *
 *  - 枚举 packages/**\/lania.package.json
 *  - 按 §6.1 校验字段、跨包唯一性、crate/npm bin 路径、describe 与 spec().name 一致
 *  - 输出按 kind 分组的清单，便于与 §8 阶段 1 的 discoverPackages() 基线对账
 */
import { discoverPackages, validateAll } from './lib/discover-packages.mjs';

const entries = discoverPackages();
const problems = validateAll(entries);

const grouped = new Map();
for (const entry of entries) {
  const kind = entry.manifest.kind ?? 'unknown';
  if (!grouped.has(kind)) grouped.set(kind, []);
  grouped.get(kind).push(entry.manifest.name);
}
for (const [kind, names] of [...grouped].sort()) {
  console.log(`  ${kind.padEnd(13)} ${names.length.toString().padStart(2)}  ${names.join(', ')}`);
}
console.log(`packages: ${entries.length} 个包，${grouped.size} 类`);

if (problems.length) {
  for (const problem of problems) console.error('  ✗ ' + problem);
  console.error(`package manifest check FAILED (${problems.length} issue(s))`);
  process.exit(1);
}
console.log('package manifest check: OK');
