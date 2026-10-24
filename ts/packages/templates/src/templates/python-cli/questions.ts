import { normalizePythonCliOptions } from './options.js';

export default ({ options = {} }: { options?: Record<string, unknown> }) => {
  const normalized = normalizePythonCliOptions(options);
  const zh = String(options.locale ?? '').toLowerCase().startsWith('zh');
  return [
    {
      name: 'projectName',
      type: 'input',
      message: zh ? '请输入项目名：' : 'Project name:',
    },
    {
      name: 'packageName',
      type: 'input',
      message: zh ? '请输入包名：' : 'Package name:',
      default: normalized.packageName,
    },
    {
      name: 'skipInstall',
      type: 'confirm',
      message: zh ? '跳过依赖安装？' : 'Skip dependency install?',
      default: true,
    },
  ];
};
