#!/usr/bin/env node
/**
 * `@lania-cli-v2/runex` 的独立 CLI 入口（文档 §3.4 主包双形态）。
 *
 * 先按 os/cpu 解析平台包里的 `runex[.exe]`，再直启二进制并转发 argv；
 * 不回退 PATH 扫描，找不到就明确报错。Rust lib API 形态由 `runex` crate 直接提供。
 */
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';

const require = createRequire(import.meta.url);

const platformKey = (() => {
  const os =
    process.platform === 'win32' ? 'win32' : process.platform === 'darwin' ? 'darwin' : 'linux';
  const cpu = process.arch === 'arm64' ? 'arm64' : 'x64';
  return `${os}-${cpu}`;
})();

let binaryPath;
try {
  const pkgName = `@lania-cli-v2/runex-${platformKey}`;
  const pkgDir = dirname(require.resolve(`${pkgName}/package.json`));
  binaryPath = join(pkgDir, 'bin', process.platform === 'win32' ? 'runex.exe' : 'runex');
} catch {
  console.error(
    `[runex] 未找到 ${platformKey} 平台包 @lania-cli-v2/runex-${platformKey}。\n` +
      '  也可设置 RUNEX_BIN 直接指向本地构建的 runex 二进制。',
  );
  process.exit(1);
}

if (process.env.RUNEX_BIN) binaryPath = process.env.RUNEX_BIN;
const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error('[runex]', result.error.message);
  process.exit(1);
}
process.exit(result.status ?? 1);
