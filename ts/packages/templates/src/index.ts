/**
 * templates 包的公共导出入口。
 *
 * 这个包主要提供两类运行时能力：
 * - create 模板（runtime templates）：用于初始化项目结构（目录级模板，含 questions/dependencies/files/outputTasks）
 * - add 模板（add-templates）：用于生成单个文件/片段（如组件、配置文件）
 *
 * node-bridge 的 template 插件会动态 import 本包（workspace 优先，其次走已安装包），
 * Rust 侧通过 bridge 方法调用这些导出函数获取模板元信息与渲染结果。
 */
export {
  getRuntimeTemplate,
  listRuntimeTemplates,
  loadTemplateDependencies,
  loadTemplateOutputTasks,
  loadTemplateQuestions,
  renderRuntimeTemplate,
} from './template-runtime.js';
export type { TemplateManifest } from './template-runtime.js';
export {
  getAddRuntimeTemplate,
  listAddRuntimeTemplates,
  renderAddRuntimeTemplate,
} from './add-template-runtime.js';
export type { AddTemplateManifest } from './add-template-runtime.js';
