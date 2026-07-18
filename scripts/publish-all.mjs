// 一键发布 lania-cli 框架的所有 npm 包
// 按依赖顺序：平台包 → CLI 主包
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function publishPackage(pkgDir, access = 'public') {
  const cwd = path.join(repoRoot, 'npm', pkgDir);
  console.log(`\n[发布] ${pkgDir} ...`);
  const result = spawnSync('npm', ['publish', '--access', access], {
    cwd,
    stdio: 'inherit',
    env: process.env,
  });
  if (result.status !== 0) {
    console.error(`[失败] ${pkgDir}`);
    process.exit(result.status ?? 1);
  }
  console.log(`[完成] ${pkgDir}`);
}

// 发布顺序：平台包先发（optionalDependency），最后发主包
publishPackage('cli-win32-x64');
publishPackage('cli-darwin-arm64');
publishPackage('cli-linux-x64');
publishPackage('cli');

console.log('\n✅ 全部发布完成');
