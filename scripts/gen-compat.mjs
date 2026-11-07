#!/usr/bin/env node
/**
 * 构建期生成兼容矩阵 `compat.json`（文档 §6.5）。
 *
 * 关系：
 * - `@lania-cli-v2/cli` → `@lania-cli-v2/core`：`dependencies: "^0.0.x"`；
 * - `core` → `cmd-*`：`api.protocol == SUPPORTED_PROTOCOL(1)`，运行时读 manifest 校验；
 * - `lan` 族 ↔ 其它三族：无版本锁，仅 API 协议约束（冻结导出面）。
 *
 * 输出：packages/distribution/core/npm/core/lib/compat.json
 * 用法：node scripts/gen-compat.mjs [--check]
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { discoverPackages } from './lib/discover-packages.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = path.join(
  repoRoot,
  'packages/distribution/core/npm/core/lib/compat.json',
);

/** core 支持的协议版本（launcher 运行时校验用同一个常量）。 */
const SUPPORTED_PROTOCOL = 1;

/**
 * 语义比较两个 JSON 产物：只关心内容，不关心字节排版。
 *
 * 产物可能被 prettier 重新格式化（短数组内联 vs 多行），那不是漂移；
 * 真正的漂移是内容不同（命令增删、字段变化）。
 */
function isSemanticDrift(currentText, expected) {
  if (!currentText) return true;
  try {
    return JSON.stringify(JSON.parse(currentText)) !== JSON.stringify(expected);
  } catch {
    return true;
  }
}

function buildCompat() {
  const packages = discoverPackages();
  const byName = new Map(packages.map((entry) => [entry.manifest.name, entry.manifest]));

  const core = byName.get('core');
  const cli = byName.get('cli');
  const commands = packages
    .filter((entry) => entry.manifest.kind === 'command')
    .map((entry) => ({
      command: entry.manifest.command,
      package: entry.manifest.npm?.name ?? null,
      protocol: entry.manifest.api?.protocol ?? 1,
      versionFamily: entry.manifest.versionFamily,
    }))
    .sort((left, right) => left.command.localeCompare(right.command));

  const families = [...new Set(packages.map((entry) => entry.manifest.versionFamily))].sort();

  return {
    protocol: SUPPORTED_PROTOCOL,
    coreVersion: core?.version ?? '0.0.0',
    relations: [
      {
        from: cli?.npm?.name ?? '@lania-cli-v2/cli',
        to: core?.npm?.name ?? '@lania-cli-v2/core',
        constraint: `^${(core?.version ?? '0.0.0').split('.').slice(0, 2).join('.')}.x`,
        reason: '总包通过 core 提供唯一 lan 入口',
      },
    ],
    commands,
    families,
    notes: [
      'lan 族 ↔ runex/agent/bridge-hub 无版本锁，仅 API 协议约束（冻结导出面）。',
      '命令包缺失时由 launcher 给出安装提示；已安装但协议版本更高时拒绝执行。',
    ],
  };
}

function main() {
  const compat = buildCompat();
  const serialized = `${JSON.stringify(compat, null, 2)}\n`;
  if (process.argv.includes('--check')) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, 'utf8') : '';
    if (isSemanticDrift(current, compat)) {
      console.error('compat.json 与包清单不一致，请运行 node scripts/gen-compat.mjs');
      process.exit(1);
    }
    console.log(`compat.json: OK（${compat.commands.length} 条命令）`);
    return;
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, serialized);
  console.log(`compat.json: 已生成 → ${path.relative(repoRoot, outputPath)}`);
}

main();
