/**
 * spa-react 模板的初始化问答项。
 *
 * 与 spa-vue 保持一致的交互策略：
 * - 前端应用才询问样式相关问题（cssProcessor/cssTools）
 * - 若 packageManager 已在外部给定，则不重复询问
 * - 若 skipGit=true，则不再询问 repository
 *
 * 这样可以让模板既支持完整交互式创建，也支持上层命令以“已决策参数”直传的非交互模式。
 */
import {
  CSS_PROCESSOR_CHOICES,
  CSS_TOOL_CHOICES,
  LINT_TOOL_CHOICES,
  PACKAGE_MANAGER_CHOICES,
  normalizeSpaReactOptions,
} from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeSpaReactOptions(options);
  const zh = String(options.locale ?? '').toLowerCase().startsWith('zh');
  const hideStyleQuestions = ['nodejs', 'toolkit'].some((item) =>
    normalized.projectType.includes(item),
  );
  const messages = {
    cssProcessor: zh ? '请选择 CSS 预处理器：' : 'Please select a css processor:',
    cssTools: zh ? '请选择 CSS 工具：' : 'Please select a css tool:',
    lintTools: zh ? '请选择 lint 工具：' : 'Please select the lint tools:',
    buildTool: zh ? '请选择构建工具：' : 'Please select a build tool:',
    packageManager: zh ? '请选择包管理器：' : 'Please select a packaging tool:',
    repository: zh ? '请输入仓库地址：' : 'Please input the repository:',
  };

  return [
    ...(hideStyleQuestions
      ? []
      : [
          {
            message: messages.cssProcessor,
            name: 'cssProcessor',
            choices: CSS_PROCESSOR_CHOICES,
            default: normalized.cssProcessor,
            type: 'list',
          },
          {
            message: messages.cssTools,
            name: 'cssTools',
            choices: CSS_TOOL_CHOICES,
            default: normalized.cssTools,
            type: 'checkbox',
          },
        ]),
    {
      message: messages.lintTools,
      name: 'lintTools',
      choices: LINT_TOOL_CHOICES,
      default: normalized.lintTools,
      type: 'checkbox',
    },
    {
      name: 'buildTool',
      message: messages.buildTool,
      choices: ['webpack', 'vite'],
      default: normalized.buildTool,
      type: 'list',
    },
    ...(normalized.packageManager
      ? []
      : [
          {
            name: 'packageManager',
            message: messages.packageManager,
            choices: PACKAGE_MANAGER_CHOICES,
            type: 'list',
          },
        ]),
    ...(normalized.skipGit
      ? []
      : [
          {
            name: 'repository',
            message: messages.repository,
            type: 'input',
          },
        ]),
  ];
};
