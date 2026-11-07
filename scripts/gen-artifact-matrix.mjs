#!/usr/bin/env node
/**
 * 构建期生成 CI 产物矩阵 `artifact-matrix.json`（文档 §6.4）。
 *
 * 由全部 manifest 派生：
 * - `lan` 族：全量 `lania-cli` + 12 个 `lan-<cmd>` + runtime，各 3 平台；
 * - `runex` 族：runex 及其平台包；
 * - `agent` / `bridge-hub` 族：独立 bin（若发 npm）。
 *
 * 关键成本约束：**绝不为每个 bin 起一个独立 job**——同一平台的多个 bin
 * 共享一次 `cargo build`（只多几次链接），矩阵粒度是「族 × 平台」。
 *
 * 输出：artifact-matrix.json（仓库根），CI 通过 `$GITHUB_OUTPUT` 注入
 * `strategy.matrix.include`。
 * 用法：node scripts/gen-artifact-matrix.mjs [--check]
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { discoverPackages } from './lib/discover-packages.mjs';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = path.join(repoRoot, 'artifact-matrix.json');

const PLATFORMS = [
  { os: 'darwin', cpu: 'arm64', suffix: 'darwin-arm64' },
  { os: 'linux', cpu: 'x64', suffix: 'linux-x64' },
  { os: 'win32', cpu: 'x64', suffix: 'win32-x64' },
];

/** 版本族 → 触发 tag 前缀（§6.5）。 */
const FAMILY_TAGS = {
  lan: 'lan-v*',
  runex: 'runex-v*',
  agent: 'agent-v*',
  'bridge-hub': 'bridge-hub-v*',
};

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

function buildMatrix() {
  const packages = discoverPackages();
  const include = [];

  for (const entry of packages) {
    const family = entry.manifest.versionFamily;
    const bin = entry.manifest.api?.rust?.bin;
    if (!bin) continue;
    const platformPackages = entry.manifest.npm?.platformPackages ?? [];
    if (platformPackages.length === 0) continue;

    for (const platform of PLATFORMS) {
      const platformPackage = platformPackages.find((name) =>
        name.endsWith(`-${platform.suffix}`),
      );
      if (!platformPackage) continue;
      include.push({
        family,
        tag: FAMILY_TAGS[family] ?? `${family}-v*`,
        package: entry.manifest.npm?.name ?? null,
        platformPackage,
        os: platform.os,
        cpu: platform.cpu,
        binary: process.platform === 'win32' && platform.os === 'win32' ? `${bin}.exe` : bin,
        artifact: `${bin}-${entry.manifest.version}-${platform.suffix}`,
        version: entry.manifest.version,
      });
    }
  }

  // 同一平台的多个 bin 合并为一个 job（共享 cargo build 缓存）。
  const jobs = new Map();
  for (const item of include) {
    const key = `${item.family}-${item.os}-${item.cpu}`;
    if (!jobs.has(key)) {
      jobs.set(key, {
        family: item.family,
        tag: item.tag,
        os: item.os,
        cpu: item.cpu,
        artifacts: [],
      });
    }
    jobs.get(key).artifacts.push(item);
  }

  return {
    protocol: 1,
    generatedFrom: 'lania.package.json',
    families: Object.keys(FAMILY_TAGS),
    jobs: [...jobs.values()].sort((left, right) =>
      `${left.family}-${left.os}-${left.cpu}`.localeCompare(`${right.family}-${right.os}-${right.cpu}`),
    ),
  };
}

function main() {
  const matrix = buildMatrix();
  const serialized = `${JSON.stringify(matrix, null, 2)}\n`;
  if (process.argv.includes('--check')) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, 'utf8') : '';
    if (isSemanticDrift(current, matrix)) {
      console.error('artifact-matrix.json 已过期，请运行 node scripts/gen-artifact-matrix.mjs');
      process.exit(1);
    }
    console.log(`artifact-matrix.json: OK（${matrix.jobs.length} 个 job）`);
    return;
  }
  fs.writeFileSync(outputPath, serialized);
  const artifacts = matrix.jobs.reduce((sum, job) => sum + job.artifacts.length, 0);
  console.log(
    `artifact-matrix.json: 已生成（${matrix.jobs.length} 个 job / ${artifacts} 个资产）`,
  );
}

main();
