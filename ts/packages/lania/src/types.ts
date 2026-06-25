import type { SchemaTools } from '@lania-cli/node-bridge';

type JsonRecord = Record<string, unknown>;

export interface ConfigEnv {
  mode: 'development' | 'production' | 'test';
  command: 'dev' | 'build' | 'pack' | 'publish' | 'runtime';
  installed: boolean;
}

export type PlatformTarget =
  | 'darwin-arm64'
  | 'darwin-x64'
  | 'linux-arm64'
  | 'linux-x64'
  | 'win32-x64'
  | string;

export interface ProductDefinition {
  name: string;
  binaryName: string;
  displayName?: string;
  version?: string;
  versionStrategy?: 'package_json' | 'fixed' | 'workspace';
  frameworkVersion?: string;
  protocolVersion?: string;
  schemaEntry?: string;
  configEntry?: string;
  templatesDir?: string;
  runtimeMode?: 'lania-hosted';
  platforms?: PlatformTarget[];
}

export interface SchemaDiscoveryConfig {
  files?: string[];
  dirs?: string[];
  allowExtensions?: string[];
}

export interface ProductConfig {
  product: ProductDefinition;
  schema?: {
    entry?: string;
    discovery?: SchemaDiscoveryConfig;
  };
  permissions?: JsonRecord;
  distribution?: JsonRecord;
  plugins?: Array<string | JsonRecord>;
}

export interface CommandArgSchema {
  name: string;
  required?: boolean;
  multiple?: boolean;
  help?: string;
}

export type CommandOptionValueKind = 'bool' | 'string' | 'number' | 'optional_string';

export interface CommandOptionSchema {
  long: string;
  short?: string | null;
  help?: string;
  valueKind?: CommandOptionValueKind;
  defaultValue?: unknown;
  choices?: Array<string | number>;
  negatable?: boolean;
  required?: boolean;
}

export interface CommandExample {
  command: string;
  description?: string;
}

export type LocalizedText = string | { zh?: string; en?: string; [key: string]: string | undefined };

export type PromptKind =
  | 'input'
  | 'select'
  | 'confirm'
  | 'multi_select'
  | 'password'
  | 'editor'
  | 'number'
  | 'fuzzy_select'
  | 'autocomplete'
  | 'search'
  | 'rawlist'
  | 'expand';

export interface PromptChoiceSchema {
  label: string;
  value: unknown;
}

export type PromptWhenSchema =
  | { type: 'equals' | 'not_equals'; key: string; value: unknown }
  | { type: 'exists' | 'truthy'; key: string };

export type PromptValidationSchema =
  | 'required'
  | { type: 'required' }
  | { type: 'min_length'; value?: number; min?: number }
  | { type: 'one_of'; values?: string[]; choices?: string[] };

export type PromptMapFunctionSchema =
  | 'trim'
  | 'lowercase'
  | 'uppercase'
  | 'to_number'
  | 'json_parse'
  | { type: 'trim' | 'lowercase' | 'uppercase' | 'to_number' | 'json_parse' }
  | { type: 'split'; separator: string };

export type PromptOnAnsweredActionSchema =
  | { type: 'set_context_value'; key: string; value: unknown }
  | {
      type: 'set_context_from_answer';
      key: string;
      field?: string;
      mapFunctions?: PromptMapFunctionSchema[];
    }
  | { type: 'goto'; target: string }
  | { type: 'goto_if'; when: PromptWhenSchema; target: string };

export interface PromptSchema {
  id?: string;
  message: LocalizedText;
  field: string;
  kind?: PromptKind;
  choices?: PromptChoiceSchema[];
  defaultValue?: unknown;
  whenMissing?: string[];
  detail?: LocalizedText;
  when?: PromptWhenSchema;
  goto?: string;
  validate?: PromptValidationSchema[];
  timeoutMs?: number;
  contextKey?: string;
  accumulation?: 'replace' | 'append';
  returnable?: boolean;
  mapFunctions?: PromptMapFunctionSchema[];
  onAnswered?: PromptOnAnsweredActionSchema[];
}

export interface TemplateVariableDefinition {
  key: string;
  type?: 'string' | 'boolean' | 'number' | 'select' | string;
  required?: boolean;
  default?: unknown;
  description?: string;
  choices?: Array<string | number>;
}

export interface TemplateDefinition {
  id: string;
  title: string;
  description?: string;
  tags?: string[];
  variables?: TemplateVariableDefinition[];
  hooks?: Record<string, unknown>;
}

export type ConditionSchema =
  | { field: string; equals: unknown }
  | { field: string; in: unknown[] }
  | { field: string; truthy: true }
  | { all: ConditionSchema[] }
  | { any: ConditionSchema[] };

export interface PresetSchema {
  about?: string;
  templateLayers?: string[];
  features?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  guards?: string[];
  postActions?: string[];
  hooks?: Record<string, unknown>;
  when?: ConditionSchema;
}

export interface FeatureSchema {
  about?: string;
  templateLayers?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  hooks?: Record<string, unknown>;
  when?: ConditionSchema;
}

export interface DependencyRecipeSchema {
  dependencies?: string[];
  devDependencies?: string[];
  scripts?: Record<string, string>;
  packageManager?: 'npm' | 'pnpm' | 'yarn' | 'bun';
  when?: ConditionSchema;
}

export interface ResolverSchema {
  from: string;
  use: string;
  when?: ConditionSchema;
}

export type MergeRuleStrategy =
  | 'replace'
  | 'append'
  | 'deep_merge'
  | 'dedupe_merge'
  | 'patch';

export interface MergeRuleSchema {
  target: string;
  strategy: MergeRuleStrategy;
  when?: ConditionSchema;
}

export type GuardSchema =
  | { type: 'directory_empty'; when?: ConditionSchema }
  | { type: 'node_version'; range: string; when?: ConditionSchema }
  | { type: 'command_exists'; command: string; when?: ConditionSchema }
  | {
      type: 'workspace_kind';
      value: 'single' | 'monorepo';
      when?: ConditionSchema;
    };

export interface PostActionSchema {
  type: PostActionType;
  message?: string;
  when?: ConditionSchema;
}

export interface ScaffoldResolverPlan {
  from: string;
  use: string;
}

export interface ScaffoldMergeRulePlan {
  name: string;
  target: string;
  strategy: MergeRuleStrategy;
}

export type PostActionType =
  | 'install_dependencies'
  | 'git_init'
  | 'git_first_commit'
  | 'print_summary';

export type ScaffoldGuardPlan =
  | { name: string; type: 'directory_empty' }
  | { name: string; type: 'node_version'; range: string }
  | { name: string; type: 'command_exists'; command: string }
  | { name: string; type: 'workspace_kind'; value: 'single' | 'monorepo' };

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
  packageManager: 'npm' | 'pnpm' | 'yarn' | 'bun' | null;
  resolvers: Record<string, ScaffoldResolverPlan>;
  mergeRules: ScaffoldMergeRulePlan[];
  guards: ScaffoldGuardPlan[];
  postActions: ScaffoldPostActionPlan[];
}

export type WorkflowStepName =
  | 'preflight'
  | 'resolvePreset'
  | 'resolveFeatures'
  | 'renderTemplates'
  | 'mergeFiles'
  | 'writeFiles'
  | 'installDependencies'
  | 'gitInit'
  | 'postActions'
  | 'printSummary';

export interface WorkflowStepDefinition {
  name: WorkflowStepName;
  options?: WorkflowStepOptions;
}

export interface WorkflowTransactionCompensationCommand {
  program: string;
  args: string[];
}

export interface WorkflowTransactionCompensationPlan {
  commands?: WorkflowTransactionCompensationCommand[];
  notes?: string[];
}

export type WorkflowTransactionRegistration =
  | { kind: 'inherit' }
  | { kind: 'none'; reason?: string }
  | { kind: 'unsupported'; reason: string; compensation?: WorkflowTransactionCompensationPlan }
  | { kind: 'compensation'; reason: string; compensation?: WorkflowTransactionCompensationPlan };

export interface WorkflowTransactionOptions {
  label?: string;
  target?: string;
  rollback?: WorkflowTransactionRegistration;
}

export type WorkflowStepOptions = JsonRecord & {
  transaction?: WorkflowTransactionOptions;
};

export interface WorkflowStepResult {
  step: WorkflowStepName;
  ok: boolean;
  data?: JsonRecord;
}

export interface WorkflowExecutionResult {
  steps: WorkflowStepResult[];
  summary: {
    templateLayers: string[];
    dependencies: string[];
    devDependencies: string[];
    scripts: Record<string, string>;
    packageManager: 'npm' | 'pnpm' | 'yarn' | 'bun' | null;
    writtenFiles?: string[];
    mergedFiles?: Array<{
      path: string;
      strategy: string;
      source: 'rendered' | 'merged';
      change: 'create' | 'replace' | 'merge';
    }>;
    postActions?: Array<Record<string, unknown>>;
    nextSteps?: string[];
    host?: {
      runtime: {
        mode: 'development' | 'installed';
        workspaceRoot: string;
        productRoot: string;
      };
      files: {
        written: string[];
        created: string[];
        merged: string[];
        replaced: string[];
      };
      packageJson: {
        dependencies: string[];
        devDependencies: string[];
        scripts: string[];
      };
      postActions: string[];
      nextSteps: string[];
      transaction: {
        applied: string[];
        rolledBack: string[];
        nonRevertible: string[];
        compensations: string[];
        rollbackFailures: string[];
        rolledBackAny: boolean;
      };
    };
    transaction?: {
      operations: Array<{
        step: string;
        target: string;
        status: 'applied' | 'skipped' | 'planned' | 'rolled_back';
        rollback:
          | 'supported'
          | 'completed'
          | 'not_needed'
          | 'not_supported'
          | 'compensation_available'
          | 'failed';
        reason?: string;
        compensation?: WorkflowTransactionCompensationPlan;
      }>;
      rolledBack: boolean;
      rollbackFailures: string[];
      nonRevertible: string[];
      compensations: string[];
    };
  };
}

export interface DeclarativeWorkflowDefinition {
  name?: string;
  guards?: string[];
  steps: Array<WorkflowStepName | WorkflowStepDefinition>;
}

export interface PluginDefinition {
  name?: string;
  package: string;
  methods?: string[];
  signature?: string;
  hooks?: Record<string, unknown>;
}

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

export interface CommandContext<
  TArgs extends JsonRecord = JsonRecord,
  TOptions extends JsonRecord = JsonRecord,
> {
  cwd: string;
  mount: string;
  path: string[];
  traceId: string | null;
  argv: {
    args: TArgs;
    options: TOptions;
  };
  tools: SchemaTools;
  scaffold: ScaffoldPlan;
  product: ProductContext;
  runtime: RuntimeContext;
}

export type CommandHandler<
  TArgs extends JsonRecord = JsonRecord,
  TOptions extends JsonRecord = JsonRecord,
> = (ctx: CommandContext<TArgs, TOptions>) => unknown | Promise<unknown>;

export interface CommandHandlerSpec {
  plugin: string;
  method: string;
}

export type WorkflowDefinition<
  TArgs extends JsonRecord = JsonRecord,
  TOptions extends JsonRecord = JsonRecord,
> =
  | CommandHandler<TArgs, TOptions>
  | CommandHandlerSpec
  | DeclarativeWorkflowDefinition;

export interface NamedWorkflowDefinition<
  TName extends string = string,
  TArgs extends JsonRecord = JsonRecord,
  TOptions extends JsonRecord = JsonRecord,
> {
  name: TName;
  handler: WorkflowDefinition<TArgs, TOptions>;
}

export type HookBindings = Record<string, unknown>;

export interface CommandSchema<
  TArgs extends JsonRecord = JsonRecord,
  TOptions extends JsonRecord = JsonRecord,
> {
  name: string;
  about?: string;
  alias?: string;
  aliases?: string[];
  args?: CommandArgSchema[];
  options?: CommandOptionSchema[];
  examples?: CommandExample[];
  prompt?: PromptSchema[];
  promptFlow?: string;
  handler?: CommandHandler<TArgs, TOptions> | CommandHandlerSpec;
  workflow?: string;
  preset?: string;
  features?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  guards?: string[];
  postActions?: string[];
  hooks?: HookBindings;
  when?: ConditionSchema;
  subcommands?: CommandSchema[];
}

export interface SchemasConfig {
  commands: CommandSchema[];
  workflows?: Record<string, WorkflowDefinition | NamedWorkflowDefinition>;
  templates?: TemplateDefinition[];
  plugins?: Array<string | PluginDefinition>;
  presets?: Record<string, PresetSchema>;
  features?: Record<string, FeatureSchema>;
  dependencyRecipes?: Record<string, DependencyRecipeSchema>;
  mergeRules?: Record<string, MergeRuleSchema>;
  guards?: Record<string, GuardSchema>;
  resolvers?: Record<string, ResolverSchema>;
  postActions?: Record<string, PostActionSchema>;
}
