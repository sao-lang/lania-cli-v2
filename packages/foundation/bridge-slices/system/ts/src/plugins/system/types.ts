// `system` 插件拆分后，这里集中放“发现命令”领域的共享类型。
// 这些类型会被 options/path-discovery/shell-discovery/service 同时消费，
// 单独抽出后可以避免子模块之间互相依赖实现文件。
export type CommandSource = 'PATH' | 'shell_builtin' | 'shell_alias' | 'shell_function';

export type DiscoveredCommand = {
  name: string;
  source: CommandSource;
  kind: 'file' | 'symlink' | 'builtin' | 'alias' | 'function';
  path?: string;
  directory?: string;
  detail?: string;
};

// ListCommandOptions 是 service 真正执行发现逻辑时使用的归一化参数。
// 原始 bridge 请求可能是弱类型结构，统一在 options 层归一化后，后续各层只处理这个稳定形状。
export type ListCommandOptions = {
  cwd: string;
  filter: string | null;
  limit: number | null;
  allMatches: boolean;
  includeShell: boolean;
  pathValue: string;
  shellExecutable: string | null;
};

// ShellDiscoverySnapshot 表示“当前 shell 探测的快照结果”，
// 它既包含探测出的 builtin/alias/function，也保留支持状态与加载状态，
// 方便 service 在结果中向上层解释为什么某些 shell 命令没有被包含进来。
export type ShellDiscoverySnapshot = {
  executable: string | null;
  shellName: string | null;
  supported: boolean;
  loaded: boolean;
  builtins: string[];
  aliases: Array<{ name: string; expansion: string }>;
  functions: string[];
};
