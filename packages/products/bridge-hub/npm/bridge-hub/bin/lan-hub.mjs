#!/usr/bin/env node
/**
 * 可选独立 `lan-hub` 调试入口（文档 §3.4 / §5.5）。
 *
 * 正式形态是进程内嵌：命令二进制静态链接 lania-bridge-host，Hub 与命令同生共死。
 * 本 bin 只在显式启用 `standalone-hub` feature 构建后才可用，用于调试与未来长驻场景。
 * 查找顺序：LANIA_HUB_BIN → 同目录平台二进制 → 明确报错（不回退 PATH 扫描）。
 */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const binary = process.env.LANIA_HUB_BIN
  ? resolve(process.env.LANIA_HUB_BIN)
  : join(here, process.platform === 'win32' ? 'lan-hub.exe' : 'lan-hub');

if (!existsSync(binary)) {
  console.error(
    '[lan-hub] 独立 Hub 二进制未安装。\n' +
      '  正式用法：命令进程内嵌 lania-bridge-host，无需本 bin。\n' +
      '  调试用法：cargo build -p lania-bridge-host --features standalone-hub --bin lan-hub\n' +
      '            然后设置 LANIA_HUB_BIN 指向产物，或把二进制放到本文件同目录。',
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error('[lan-hub]', result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
