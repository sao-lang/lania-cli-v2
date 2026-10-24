/**
 * 按构建工具选择对应 compiler adapter 的统一入口。
 *
 * 主要导出：getCompilerAdapters、resolveCompilerAdapter。
 */
import type { BuildTool, CompilerAdapter } from '../compiler-shared.js';
import { nestCompilerAdapter, tscCompilerAdapter } from './node-framework.js';
import { rollupCompilerAdapter } from './rollup.js';
import { viteCompilerAdapter } from './vite.js';
import { webpackCompilerAdapter } from './webpack.js';

const compilerAdapters: CompilerAdapter[] = [
  viteCompilerAdapter,
  webpackCompilerAdapter,
  rollupCompilerAdapter,
  nestCompilerAdapter,
  tscCompilerAdapter,
];

export function getCompilerAdapters(): CompilerAdapter[] {
  return compilerAdapters.slice();
}

export function resolveCompilerAdapter(tool: BuildTool): CompilerAdapter | null {
  return compilerAdapters.find((adapter) => adapter.tool === tool) ?? null;
}
