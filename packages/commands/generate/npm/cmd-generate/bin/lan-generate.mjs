#!/usr/bin/env node
/**
 * `@lania-cli-v2/cmd-generate` 的 npm bin（文档 §5.3 C.7 / §5.4 D.3）。
 *
 * core launcher 的正式路径是**直启 Rust 二进制** `lan-generate[.exe]`；本 wrapper 只供
 * `npx lan-generate ...` 或用户把 `lan-generate` 直接放进 PATH 时用，两条路径最终抵达同一份
 * `lania_cli_kit::run_single_command`，行为一致。argv 契约：命令名已被剥掉。
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

let binaryPath = process.env.LAN_GENERATE_BIN;
if (!binaryPath) {
  try {
    const pkg = `@lania-cli-v2/cmd-generate-${platformKey}`;
    const pkgDir = dirname(require.resolve(`${pkg}/package.json`));
    binaryPath = join(
      pkgDir,
      'bin',
      process.platform === 'win32' ? 'lan-generate.exe' : 'lan-generate',
    );
  } catch {
    console.error(
      `[lan-generate] 未找到 ${platformKey} 平台包 @lania-cli-v2/cmd-generate-${platformKey}。\n` +
        '  可设置 LAN_GENERATE_BIN 指向本地构建的 lan-generate 二进制。',
    );
    process.exit(1);
  }
}

const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error('[lan-generate]', result.error.message);
  process.exit(1);
}
if (typeof result.status === 'number') {
  process.exit(result.status);
}
process.exit(result.signal ? 1 : 0);
