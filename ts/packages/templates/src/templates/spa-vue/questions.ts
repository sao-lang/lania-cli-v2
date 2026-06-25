/**
 * spa-vue 模板的初始化问答项。
 *
 * 这里的返回值会被 template-runtime.ts 读取并标准化为“可序列化的 questions 数组”。
 * 注意：不同交互库对字段命名略有差异（name/type/choices/default/message），因此这里尽量保持简单。
 *
 * 隐藏问题项的规则：
 * - 当 projectType 更偏向“非前端应用”（例如 nodejs/toolkit）时，隐藏 cssProcessor/cssTools 等样式相关问题。
 * - 当 packageManager/skipGit 已由 options 给出时，避免重复问用户。
 */
import {
  CSS_PROCESSOR_CHOICES,
  CSS_TOOL_CHOICES,
  LINT_TOOL_CHOICES,
  PACKAGE_MANAGER_CHOICES,
  normalizeSpaVueOptions,
} from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeSpaVueOptions(options);
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
