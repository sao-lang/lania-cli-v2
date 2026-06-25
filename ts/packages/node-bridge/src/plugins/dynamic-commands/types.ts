import type { SchemaTools } from '../../core/schema-tools.js';

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

export type MergeRuleStrategy =
  | 'replace'
  | 'append'
  | 'deep_merge'
  | 'dedupe_merge'
  | 'patch';

export interface ScaffoldMergeRulePlan {
  name: string;
  target: string;
  strategy: MergeRuleStrategy;
}

export type GuardType =
  | 'directory_empty'
  | 'node_version'
  | 'command_exists'
  | 'workspace_kind';

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

export type WorkflowStepOptions = JsonObject & {
  transaction?: WorkflowTransactionOptions;
};

export interface WorkflowStepResult {
  step: WorkflowStepName;
  ok: boolean;
  data?: JsonObject;
}

export interface WorkflowExecutionResult {
  steps: WorkflowStepResult[];
  summary: {
    templateLayers: string[];
    dependencies: string[];
    devDependencies: string[];
    scripts: Record<string, string>;
    packageManager: string | null;
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
        compensation?: {
          commands?: Array<{ program: string; args: string[] }>;
          notes?: string[];
        };
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

export type DynamicHandlerFn = (ctx: DynamicCommandContext) => unknown | Promise<unknown>;

/**
 * 动态命令执行上下文。
 * Rust 已完成 argv 结构化解析，Node 侧只消费 args/options 对象。
 */
export interface DynamicCommandContext {
  cwd: string;
  mount: string;
  path: string[];
  argv: { args: JsonObject; options: JsonObject };
  traceId: string | null;
  tools: SchemaTools;
  scaffold: ScaffoldPlan;
  product: ProductContext;
  runtime: RuntimeContext;
}

export type HookKind = 'waterfall' | 'parallel';

export type HookBinding =
  | {
      type: 'plugin';
      kind?: HookKind;
      plugin: string;
      handler: string;
      timeoutMs?: number;
      onError?: 'throw' | 'collect';
    }
  | {
      // Inline hook functions cannot be serialized; Rust only holds an id and calls back to Node.
      type: 'inline';
      kind?: HookKind;
      id: string;
    };

export type HookBindings = Record<string, HookBinding[]>;

export type InlineHookFn = (
  payload: unknown,
  ctx: { cwd: string; hook: string; kind: string; source: string },
) => unknown | Promise<unknown>;

interface PluginMethodExecutor {
  type: 'plugin';
  plugin: string;
  method: string;
}

interface LocalFunctionExecutor {
  type: 'local';
  id: string;
}

export type CommandExecutor = PluginMethodExecutor | LocalFunctionExecutor;

export const RESERVED_TOP_LEVEL_COMMANDS = new Set([
  'dev',
  'build',
  'lint',
  'create',
  'add',
  'sync',
  'template',
  'release',
  'generate',
  'help',
]);

export const DEFAULT_SCHEMA_DISCOVERY = {
  files: ['lania.schemas.ts', 'lania.schemas.js', 'lania.schemas.cjs'],
  dirs: ['.lania/schemas'],
  // Keep in sync with the actual loader support (runtime.ts). TOML is not supported yet.
  allowExtensions: ['.ts', '.js', '.cjs', '.json', '.yaml', '.yml'],
};

export interface RuntimeManifest {
  runtimeCommands: RuntimeCommandConfig[];
  commands: CommandDeclaration[];
  workflows: Record<string, WorkflowDefinition>;
  templates: TemplateDefinition[];
  plugins: RuntimePluginReference[];
  presets: Record<string, PresetDefinition>;
  features: Record<string, FeatureDefinition>;
  dependencyRecipes: Record<string, DependencyRecipeDefinition>;
  mergeRules: Record<string, MergeRuleDefinition>;
  guards: Record<string, GuardDefinition>;
  resolvers: Record<string, ResolverDefinition>;
  postActions: Record<string, PostActionDefinition>;
}

interface RuntimeCommandConfig {
  mount: string;
  runtime?: Record<string, unknown>;
  options?: Record<string, unknown>;
  command?: CommandPackMeta;
  commands: CommandDeclaration[];
  // Optional compatibility path: external converter plugin transforms schemas -> commands.
  schemas?: RuntimeSchemaDeclaration[];
}

export interface RuntimeSchemaDeclaration {
  plugin?: string;
  method?: string;
  kind?: string;
  source?: string;
  [key: string]: unknown;
}

export interface TemplateVariableDefinition {
  key: string;
  type?: string;
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

export interface PresetDefinition {
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

export interface FeatureDefinition {
  about?: string;
  templateLayers?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  hooks?: Record<string, unknown>;
  when?: ConditionSchema;
}

export interface DependencyRecipeDefinition {
  dependencies?: string[];
  devDependencies?: string[];
  scripts?: Record<string, string>;
  packageManager?: string;
  when?: ConditionSchema;
}

export interface ResolverDefinition {
  from: string;
  use: string;
  when?: ConditionSchema;
}

export interface MergeRuleDefinition {
  target: string;
  strategy: MergeRuleStrategy;
  when?: ConditionSchema;
}

export type GuardDefinition =
  | { type: 'directory_empty'; when?: ConditionSchema }
  | { type: 'node_version'; range: string; when?: ConditionSchema }
  | { type: 'command_exists'; command: string; when?: ConditionSchema }
  | {
      type: 'workspace_kind';
      value: 'single' | 'monorepo';
      when?: ConditionSchema;
    };

export interface PostActionDefinition {
  type: PostActionType;
  message?: string;
  when?: ConditionSchema;
}

export interface RuntimePluginReference {
  name?: string;
  package: string;
  methods?: string[];
  signature?: string;
  hooks?: Record<string, unknown>;
}

export interface ResolveDynamicResult {
  commands: GeneratedCommandSpec[];
  handlers: DynamicHandlerRegistration[];
  mounts: Array<{ mount: string; rootHandlerId: string }>;
  warnings: string[];
}

type ValueKind = 'bool' | 'string' | 'number' | 'optional_string';

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

export interface PromptChoiceSpec {
  label: string;
  value: unknown;
}

export type PromptWhenSpec =
  | { type: 'equals' | 'not_equals'; key: string; value: unknown }
  | { type: 'exists' | 'truthy'; key: string };

export type PromptValidationSpec =
  | 'required'
  | { type: 'required' }
  | { type: 'min_length'; value?: number; min?: number }
  | { type: 'one_of'; values?: string[]; choices?: string[] };

export type PromptMapFunctionSpec =
  | 'trim'
  | 'lowercase'
  | 'uppercase'
  | 'to_number'
  | 'json_parse'
  | { type: 'trim' | 'lowercase' | 'uppercase' | 'to_number' | 'json_parse' }
  | { type: 'split'; separator: string };

export type PromptOnAnsweredActionSpec =
  | { type: 'set_context_value'; key: string; value: unknown }
  | {
      type: 'set_context_from_answer';
      key: string;
      field?: string;
      mapFunctions?: PromptMapFunctionSpec[];
    }
  | { type: 'goto'; target: string }
  | { type: 'goto_if'; when: PromptWhenSpec; target: string };

export interface PromptSpec {
  id?: string;
  // Supports localized message payload: `{ zh: "...", en: "..." }`.
  message: LocalizedText;
  field: string;
  kind?: PromptKind;
  choices?: PromptChoiceSpec[];
  defaultValue?: unknown;
  whenMissing?: string[];
  // Optional prompt detail (shown as extra hint). Also supports i18n object.
  detail?: LocalizedText;
  when?: PromptWhenSpec;
  goto?: string;
  validate?: PromptValidationSpec[];
  timeoutMs?: number;
  contextKey?: string;
  accumulation?: 'replace' | 'append';
  returnable?: boolean;
  mapFunctions?: PromptMapFunctionSpec[];
  onAnswered?: PromptOnAnsweredActionSpec[];
}

export interface CommandHandlerSpec {
  plugin: string;
  method: string;
}

export type WorkflowDefinition =
  | CommandHandlerSpec
  | DynamicHandlerFn
  | DeclarativeWorkflowDefinition;

export interface CommandDeclaration {
  name: string;
  about?: string;
  alias?: string;
  aliases?: string[];
  args?: GeneratedCommandSpec['args'];
  options?: Array<GeneratedCommandSpec['options'][number] & { required?: boolean }>;
  examples?: GeneratedCommandSpec['examples'];
  prompt?: PromptSpec[];
  promptFlow?: string;
  handler?: CommandHandlerSpec | DynamicHandlerFn;
  workflow?: string;
  preset?: string;
  features?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  guards?: string[];
  postActions?: string[];
  hooks?: HookBindings;
  plugin?: unknown;
  when?: ConditionSchema;
  subcommands?: CommandDeclaration[];
}

export interface ManifestCommandScaffoldSpec {
  preset?: string;
  features?: string[];
  dependencyRecipes?: string[];
  mergeRules?: string[];
  guards?: string[];
  postActions?: string[];
  presets?: Record<string, PresetDefinition>;
  featuresCatalog?: Record<string, FeatureDefinition>;
  dependencyRecipeCatalog?: Record<string, DependencyRecipeDefinition>;
  mergeRulesCatalog?: Record<string, MergeRuleDefinition>;
  guardsCatalog?: Record<string, GuardDefinition>;
  resolvers?: Record<string, ResolverDefinition>;
  postActionsCatalog?: Record<string, PostActionDefinition>;
}

export interface CommandPackMeta {
  about?: string;
  alias?: string;
  aliases?: string[];
  examples?: GeneratedCommandSpec['examples'];
}

export interface GeneratedCommandSpec {
  name: string;
  about: string;
  alias: string | null;
  aliases: string[];
  args: Array<{
    name: string;
    required: boolean;
    multiple: boolean;
    help: string;
  }>;
  options: Array<{
    long: string;
    short: string | null;
    help: string;
    value_kind: ValueKind;
    default_value: string | null;
    choices: string[];
    negatable: boolean;
  }>;
  examples: Array<{
    command: string;
    description: string;
  }>;
  subcommands: GeneratedCommandSpec[];
  handler_id: string;
}

export interface DynamicHandlerRegistration {
  handlerId: string;
  method: 'command.invokeDynamic';
  plugin: 'dynamic-commands';
  target: DynamicInvocationTarget;
}

export type DynamicInvocationTarget =
  | {
      kind: 'mount_summary';
      mount: string;
      commands: string[];
    }
  | {
      kind: 'group_summary';
      mount: string;
      path: string[];
      commands: string[];
    }
  | {
      kind: 'manifest_command';
      mount: string;
      path: string[];
      schemaRoot: string;
      declaredPlugins?: RuntimePluginReference[];
      executor: CommandExecutor;
      prompt?: PromptSpec[];
      requiredOptions?: string[];
      hooks?: HookBindings;
      scaffold?: ManifestCommandScaffoldSpec;
    };
