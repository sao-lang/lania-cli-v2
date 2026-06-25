/**
 * toolkit 模板的初始化问答项。
 *
 * 约定：
 * - lintTools 默认给出一组常用组合（由 normalizeToolkitOptions 决定），用户可按需取消。
 * - unitTestTool 允许选择 "skip"，用于生成一个更轻量的工具库项目。
 * - packageManager/repository 在已提供（或 skipGit=true）时不重复询问。
 */
import {
  LINT_TOOL_CHOICES,
  PACKAGE_MANAGER_CHOICES,
  UNIT_TEST_TOOL_CHOICES,
  normalizeToolkitOptions,
} from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeToolkitOptions(options);
  const zh = String(options.locale ?? '').toLowerCase().startsWith('zh');
  const messages = {
    lintTools: zh ? '请选择 lint 工具：' : 'Please select the lint tools:',
    unitTestTool: zh ? '请选择单元测试工具：' : 'Please select a unit testing tool',
    packageManager: zh ? '请选择包管理器：' : 'Please select a packaging tool:',
    repository: zh ? '请输入仓库地址：' : 'Please input the repository:',
  };

  return [
    {
      message: messages.lintTools,
      name: 'lintTools',
      choices: LINT_TOOL_CHOICES,
      type: 'checkbox',
      // 默认值来自 normalizeToolkitOptions，保证交互式与非交互式调用行为一致。
      default: normalized.lintTools,
    },
    {
      message: messages.unitTestTool,
      name: 'unitTestTool',
      choices: [...UNIT_TEST_TOOL_CHOICES, 'skip'],
      type: 'list',
      // `skip` 会让 dependencies/config 层不再生成测试相关能力。
      default: normalized.unitTestTool ?? 'skip',
    },
    ...(normalized.packageManager
      ? []
      : [
          {
            name: 'packageManager',
            message: messages.packageManager,
            choices: PACKAGE_MANAGER_CHOICES,
            type: 'list',
            default: 'pnpm',
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
