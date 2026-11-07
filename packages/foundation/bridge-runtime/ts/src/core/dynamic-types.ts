/**
 * 动态命令域的共享上下文类型（阶段 4b 拆包的核侧单一事实源）。
 *
 * 这些类型同时被核（schema-tools / scaffold）与 `bridge-slice-dynamic-commands`、
 * `bridge-slice-template` 消费，因此必须定义在核里；切片侧从
 * `@lania-cli-v2/bridge-runtime/core/dynamic-types.js` 导入并按需再导出。
 */

/** A JSON-like object received from Rust or manifest parsing helpers. */
type JsonObject = Record<string, unknown>;

type LocalizedText = string | { zh?: string; en?: string; [key: string]: string | undefined };

export interface RuntimeContext {
  mode: 'development' | 'installed';
  traceId: string | null;
  invocationCwd: string;
  workspaceRoot: string;
  productRoot: string;
  schemaRoot: string;
}

export interface ProductContext {
  name: string;
  binaryName: string;
  displayName: string | null;
  version: string | null;
  productRoot: string;
  schemaRoot: string;
  templatesDir: string | null;
}

export interface ScaffoldResolverPlan {
  from: string;
  use: string;
}

export type MergeRuleStrategy = 'replace' | 'append' | 'deep_merge' | 'dedupe_merge' | 'patch';

export interface ScaffoldMergeRulePlan {
  name: string;
  target: string;
  strategy: MergeRuleStrategy;
}

export type GuardType = 'directory_empty' | 'node_version' | 'command_exists' | 'workspace_kind';

export type ScaffoldGuardPlan =
  | { name: string; type: 'directory_empty' }
  | { name: string; type: 'node_version'; range: string }
  | { name: string; type: 'command_exists'; command: string }
  | { name: string; type: 'workspace_kind'; value: 'single' | 'monorepo' };

export type PostActionType =
  | 'install_dependencies'
  | 'git_init'
  | 'git_first_commit'
  | 'print_summary';

export interface ScaffoldPostActionPlan {
  name: string;
  type: PostActionType;
  message?: string;
}

export interface ScaffoldPlan {
  preset: string | null;
  features: string[];
  templateLayers: string[];
  dependencyRecipes: string[];
  dependencies: string[];
  devDependencies: string[];
  scripts: Record<string, string>;
  packageManager: string | null;
  resolvers: Record<string, ScaffoldResolverPlan>;
  mergeRules: ScaffoldMergeRulePlan[];
  guards: ScaffoldGuardPlan[];
  postActions: ScaffoldPostActionPlan[];
}

export type { JsonObject, LocalizedText };
