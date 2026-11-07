#!/usr/bin/env node
/**
 * 把 `protocol/gen/ts/*` 的生成产物拷进本包的 src/（生成物不入库，编译时同步）。
 *
 * 生成物带 `.ts` 扩展名的相对导入，tsconfig 用
 * `allowImportingTsExtensions` + `rewriteRelativeImportExtensions` 处理，
 * 编译产物里自动改写为 `.js`。
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(here, '..');
const genDir = path.resolve(packageRoot, '../../gen/ts');
const srcDir = path.join(packageRoot, 'src');

fs.mkdirSync(srcDir, { recursive: true });
for (const name of fs.readdirSync(genDir)) {
  if (!name.endsWith('.ts')) continue;
  fs.copyFileSync(path.join(genDir, name), path.join(srcDir, name));
}
fs.writeFileSync(
  path.join(srcDir, 'index.ts'),
  [
    '// 由 scripts/copy-gen.mjs 从 protocol/gen/ts 生成物同步，请勿手改。',
    "export * from './protocol.ts';",
    "export * from './runtime.ts';",
    '',
  ].join('\n'),
  'utf8',
);
console.log('[bridge-protocol] 生成物已同步');
