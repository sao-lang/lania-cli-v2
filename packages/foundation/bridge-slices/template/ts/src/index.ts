/**
 * bridge 切片：template ejs 模板渲染切片。
 *
 * 主要导出：plugin。
 *
 * 由核 `@lania-cli-v2/bridge-runtime` 的 SLICE_LOADERS 按
 * `LANIA_BRIDGE_SLICES` 清单懒加载（阶段 4b，§5.2 B.4）。
 */
export { templatePlugin as plugin } from './plugins/template.js';
