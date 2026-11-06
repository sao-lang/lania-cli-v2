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
/**
 * 运行时 → 模板提问选项的唯一来源（设计文档 B14）。
 *
 * 模板内部通过相对路径读取；这里对外导出，方便 bridge / 未来模板面板复用同一份数据
 * （`node-bridge` 走 `./presentation` 子路径 import，避免把整个模板运行时打进 lint 路径）。
 */
export { RUNTIME_PRESENTATIONS, choicesFor, presentationRuntimeIds } from './presentation.js';
/**
 * node 模板族的「额外工程化工具」选项（B14 extras）：
 * spa-react / spa-vue / toolkit / toolkit-monorepo 的 lint 工具、包管理器与 git hook 名单
 * 都从这一份派生（`NODE_TEMPLATE_CHOICES` + `NODE_GIT_HOOK_TOOLS`）。
 */
export { NODE_GIT_HOOK_TOOLS, NODE_TEMPLATE_CHOICES, templateChoicesFor } from './presentation.js';
export type { TemplateChoiceProfile, TemplateToolChoices } from './presentation.js';
export type { ChoiceOption, ChoiceSpec } from './language-options.js';
