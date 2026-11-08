#!/usr/bin/env node
// 开发态试用辅助：把 `cargo build --workspace --bins` 的产物 staging 到各命令子包
// 的 npm 平台包 bin/ 目录，让 launcher（`node packages/distribution/core/npm/core/bin/lan.mjs
// <cmd>`）能按发布形态的路径调度到它们。
//
// 用法：
//   node ./scripts/stage-dev-binaries.mjs            # staging 全部已构建的命令二进制
//   node ./scripts/stage-dev-binaries.mjs --list     # 只打印映射，不拷贝
//   node ./scripts/stage-dev-binaries.mjs env git    # 只 staging 指定命令
//
// 说明：target/debug 是 debug 构建，仅供本地试用；发布产物的二进制由
// `scripts/pack.mjs` / CI release 构建填充，不依赖本脚本。

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const repoRoot = process.cwd();
const knownCommandsPath = path.join(
  repoRoot,
  'packages/distribution/core/npm/core/lib/known-commands.json',
);
const known = JSON.parse(fs.readFileSync(knownCommandsPath, 'utf8'));
const platformDirSuffix = `${process.platform}-${process.arch}`;
const exeSuffix = process.platform === 'win32' ? '.exe' : '';

const args = process.argv.slice(2);
const listOnly = args.includes('--list');
const requested = args.filter((item) => !item.startsWith('--'));

let staged = 0;
const missing = [];

for (const entry of known.commands) {
  const binName = entry.bin ?? entry.binaryName;
  if (!binName) continue;
  if (requested.length > 0 && !requested.includes(entry.command)) continue;

  // 包名 `@lania-cli-v2/cmd-env` → npm 目录 `packages/commands/env/npm/cmd-env-<platform>`
  const scopeless = entry.package.split('/')[1];
  const npmDir = path.join(repoRoot, `packages/commands/${entry.command}/npm`, `${scopeless}-${platformDirSuffix}`);
  const source = path.join(repoRoot, 'target/debug', `${binName}${exeSuffix}`);
  const target = path.join(npmDir, 'bin', `${binName}${exeSuffix}`);

  if (listOnly) {
    const state = fs.existsSync(source) ? 'ready' : 'not built';
    console.log(`[stage] ${entry.command.padEnd(8)} ${binName}${exeSuffix.padEnd(18)} ${state}`);
    continue;
  }

  if (!fs.existsSync(source)) {
    missing.push(entry.command);
    continue;
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
  staged += 1;
  console.log(`[stage] ${entry.command} → ${path.relative(repoRoot, target)}`);
}

if (listOnly) process.exit(0);

if (missing.length > 0) {
  console.warn(
    `[stage] 未构建的命令（先跑 cargo build --workspace --bins）：${missing.join(', ')}`,
  );
}
console.log(`[stage] 完成：${staged} 个二进制已就位。试用：`);
console.log(
  '  node packages/distribution/core/npm/core/bin/lan.mjs <command> [args...]',
);
