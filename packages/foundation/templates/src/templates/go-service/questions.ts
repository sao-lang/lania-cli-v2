import { buildChoiceQuestions } from '../../language-options.js';
import { GO_CHOICES, normalizeGoServiceOptions } from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizeGoServiceOptions(options);
  const zh = String(options.locale ?? '').toLowerCase().startsWith('zh');
  return [
    {
      name: 'projectName',
      type: 'input',
      message: zh ? '请输入项目名：' : 'Project name:',
    },
    {
      name: 'modulePath',
      type: 'input',
      message: zh ? '请输入 Go module 路径：' : 'Go module path:',
      default: normalized.modulePath,
    },
    ...buildChoiceQuestions(GO_CHOICES, options, zh),
    {
      name: 'skipInstall',
      type: 'confirm',
      message: zh ? '跳过依赖安装？' : 'Skip dependency install?',
      default: true,
    },
  ];
};
