/**
 * lint 插件的稳定外部入口。
 *
 * 对外仍保持 `../plugins/lint.js` 这个导入路径不变，
 * 具体实现拆到 `./lint/*` 子模块中。
 */
export { lintPlugin } from './lint/plugin.js';
