#!/usr/bin/env node
/**
 * `@lania-cli-v2/cmd-git` 的 npm bin（文档 §5.3 C.7 / §5.4 D.3）。
 *
 * core launcher 的正式路径是**直启 Rust 二进制** `lan-git[.exe]`；本 wrapper 只供
 * `npx lan-git ...` 或用户把 `lan-git` 直接放进 PATH 时用，两条路径最终抵达同一份
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

let binaryPath = process.env.LAN_GIT_BIN;
if (!binaryPath) {
  try {
    const pkg = `@lania-cli-v2/cmd-git-${platformKey}`;
    const pkgDir = dirname(require.resolve(`${pkg}/package.json`));
    binaryPath = join(pkgDir, 'bin', process.platform === 'win32' ? 'lan-git.exe' : 'lan-git');
  } catch {
    console.error(
      `[lan-git] 未找到 ${platformKey} 平台包 @lania-cli-v2/cmd-git-${platformKey}。\n` +
        '  可设置 LAN_GIT_BIN 指向本地构建的 lan-git 二进制。',
    );
    process.exit(1);
  }
}

const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error('[lan-git]', result.error.message);
  process.exit(1);
}
if (typeof result.status === 'number') {
  process.exit(result.status);
}
process.exit(result.signal ? 1 : 0);
