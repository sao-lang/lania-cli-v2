#!/usr/bin/env node
/**
 * 构建期生成 `known-commands.json`（文档 §5.3 C.7）。
 *
 * 输入：所有 `kind === "command"` 的 `lania.package.json`（命令子包清单）。
 * 输出：`packages/distribution/core/npm/core/lib/known-commands.json`
 *       —— 随 `@lania-cli-v2/core` 发布，运行时 launcher 只读此表，
 *          **不扫描目录、不发现未知命令**。
 *
 * 用法：node scripts/gen-known-commands.mjs [--check]
 *   --check：只校验输出是否为最新（CI 用，漂移即失败）。
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = path.join(
  repoRoot,
  'packages/distribution/core/npm/core/lib/known-commands.json',
);

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

/** 递归收集 packages/ 下所有 lania.package.json。 */
function collectManifests(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'target' || entry.name === 'dist') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      collectManifests(full, out);
    } else if (entry.name === 'lania.package.json') {
      out.push(full);
    }
  }
  return out;
}

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

function buildKnownCommands() {
  const packagesDir = path.join(repoRoot, 'packages');
  const manifests = collectManifests(packagesDir)
    .map((file) => ({ file, manifest: readJson(file) }))
    .filter(({ manifest }) => manifest.kind === 'command');

  const commands = manifests
    .map(({ file, manifest }) => {
      const dir = path.dirname(file);
      const npm = manifest.npm ?? {};
      const describeRel = manifest.describe ?? null;
      const describe = describeRel
        ? JSON.parse(fs.readFileSync(path.join(dir, describeRel), 'utf8'))
        : null;
      const platformPackages = (npm.platformPackages ?? []).slice();
      const binName = Object.keys(npm.bin ?? {})[0] ?? null;
      const payload = manifest.payload ?? {};
      return {
        command: manifest.command,
        package: npm.name,
        // bin 是二进制文件名（含 lan- 前缀），npmBin 是 npm 包里的 .mjs wrapper。
        bin: binName,
        npmBin: binName ? `${binName}.mjs` : null,
        order: manifest.order,
        describe: describeRel,
        about: describe?.about ?? null,
        needsNodeBridge: payload.nodeBridge === true,
        slices: Array.isArray(payload.slices) ? payload.slices : [],
        platformPackages,
        protocol: manifest.api?.protocol ?? 1,
        // 各子包声明的包级配置键。launcher 的 `lan config packages/apply` 只读这份
        // 静态表即可完成跨包管理，不必在运行时再去解析已安装 npm 包的 manifest。
        configSchema: manifest.configSchema ?? null,
      };
    })
    .sort((a, b) => (a.order ?? 99) - (b.order ?? 99));

  return {
    protocol: 1,
    coreVersion: readJson(path.join(repoRoot, 'packages/distribution/core/npm/core/package.json'))
      .version,
    commands,
  };
}

function main() {
  const known = buildKnownCommands();
  const serialized = `${JSON.stringify(known, null, 2)}\n`;

  if (process.argv.includes('--check')) {
    // 语义比较而非字节比较：产物可能被 prettier 重新格式化（数组内联 vs 多行），
    // 那不是漂移。真正的漂移是内容不同（命令增删/字段变化）。
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, 'utf8') : '';
    if (isSemanticDrift(current, known)) {
      console.error(
        'known-commands.json 与包清单不一致，请运行 node scripts/gen-known-commands.mjs',
      );
      process.exit(1);
    }
    console.log(`known-commands.json: OK（${known.commands.length} 条命令）`);
    return;
  }

  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, serialized);
  console.log(
    `known-commands.json: 已生成（${known.commands.length} 条命令）→ ${path.relative(repoRoot, outputPath)}`,
  );
}

main();
