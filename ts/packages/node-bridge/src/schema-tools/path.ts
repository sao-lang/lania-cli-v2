/**
 * `tools.path`：对 Node `path` 常用能力的直接透传。
 *
 * 这个模块几乎不引入额外语义，存在的主要原因是让 schema 在 `ctx.tools`
 * 里拿到一组稳定、可预期的路径操作函数，而不需要直接依赖 Node 全局模块。
 */
import path from 'node:path';

export interface PathTools {
  join: (...parts: string[]) => string;
  resolve: (...parts: string[]) => string;
  dirname: (p: string) => string;
  basename: (p: string, ext?: string) => string;
  relative: (from: string, to: string) => string;
  extname: (p: string) => string;
  normalize: (p: string) => string;
  isAbsolute: (p: string) => boolean;
}

export function createPathTools(): PathTools {
  // 全部方法都是纯函数透传，不附带 cwd、策略校验或 host 交互语义。
  return {
    join: (...parts) => path.join(...parts),
    resolve: (...parts) => path.resolve(...parts),
    dirname: (p) => path.dirname(p),
    basename: (p, ext) => path.basename(p, ext),
    relative: (from, to) => path.relative(from, to),
    extname: (p) => path.extname(p),
    normalize: (p) => path.normalize(p),
    isAbsolute: (p) => path.isAbsolute(p),
  };
}
