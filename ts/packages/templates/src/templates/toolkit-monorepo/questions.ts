/**
 * toolkit-monorepo 模板的初始化问答项。
 *
 * 这里尽量只问 monorepo “必要且少量”的问题：
 * - lintTools：决定是否生成对应配置文件与 git hooks
 * - unitTestTool：目前固定为 vitest（后续可扩展）
 * - packageManager：未指定时默认引导用户选择 pnpm（与 monorepo 模板更契合）
 */
import { normalizeToolkitMonorepoOptions } from './options.js';

const LINT_TOOL_CHOICES = [
  'eslint',
  'oxlint',
  'prettier',
  'oxfmt',
  'commitlint',
  'editorconfig',
];
const PACKAGE_MANAGER_CHOICES = ['pnpm', 'npm', 'yarn', 'bun'];

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitMonorepoOptions(options);
  const zh = String(options.locale ?? '').toLowerCase().startsWith('zh');
  const messages = {
    lintTools: zh ? '请选择 lint 工具：' : 'Please select the lint tools:',
    unitTestTool: zh ? '请选择测试运行器：' : 'Please select the test runner:',
    packageManager: zh ? '请选择包管理器：' : 'Please select the package manager:',
  };

  return [
    {
      message: messages.lintTools,
      name: 'lintTools',
      choices: LINT_TOOL_CHOICES,
      type: 'checkbox',
      // 默认值与 normalizeToolkitMonorepoOptions 保持一致，避免交互式模式与直传参数模式不一致。
      default: normalized.lintTools,
    },
    {
      message: messages.unitTestTool,
      name: 'unitTestTool',
      choices: ['vitest'],
      type: 'select',
      default: normalized.unitTestTool ?? undefined,
    },
    ...(normalized.packageManager
      ? []
      : [
          {
            message: messages.packageManager,
            name: 'packageManager',
            choices: PACKAGE_MANAGER_CHOICES,
            type: 'select',
            default: 'pnpm',
          },
        ]),
  ];
};
