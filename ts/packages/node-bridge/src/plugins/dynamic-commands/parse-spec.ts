/**
 * 负责把 dynamic-commands 的 manifest 片段整理成运行时真正消费的两类产物：
 * 1. 给 Rust 主进程使用的 command spec，用来描述命令形状、帮助信息和参数结构。
 * 2. 给 Node 侧使用的 handler 注册表，用来保留执行入口、hook 和 scaffold 上下文。
 *
 * 这里有一个刻意保持的约束：
 * 解析阶段尽量宽松，尽可能跳过局部坏数据；注册阶段尽量稳定，保持既有 handler id
 * 和执行协议不变。这样 manifest 可以渐进演进，而不会因为某一条命令配置不完整就把
 * 其他命令一并拖垮。
 */
import { finalizeInlineHookBindings, mergeHooks, parseHookBindings } from './hooks.js';
import { parsePrompt } from './parse-prompt.js';
import {
  createCommandSpec,
  normalizeValueKind,
  parseExamples,
  sanitizeSegment,
  stringArray,
} from './parse-shared.js';
import { executeDeclarativeWorkflow } from './workflow-steps.js';
import { registerLocalExecutor } from './state.js';
import type {
  CommandDeclaration,
  CommandExecutor,
  CommandHandlerSpec,
  DeclarativeWorkflowDefinition,
  DynamicHandlerFn,
  DynamicHandlerRegistration,
  GeneratedCommandSpec,
  RuntimePluginReference,
  TemplateDefinition,
  TemplateVariableDefinition,
  WorkflowDefinition,
} from './types.js';
import { asRecord } from '../../core/runtime.js';

/**
 * 解析 manifest 里的 `commands` 数组，产出规范化后的命令声明树。
 *
 * 这个阶段只做数据清洗和结构归一化，尽量保留作者原始意图，不会在这里解析 workflow、
 * 绑定执行器，或生成 handler id。那些动作依赖 mount 路径和运行时元数据，只能放到
 * 后面的编译阶段处理。
 */
export function parseCommands(cwd: string, value: unknown): CommandDeclaration[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const results: CommandDeclaration[] = [];
  for (const item of value) {
    const record = asRecord(item);
    const name = typeof record.name === 'string' ? sanitizeSegment(record.name) : '';
    if (!name) {
      continue;
    }

    const handlerSpec = parseExecutable(record.handler);

    const pluginRef = record.plugin;
    const pluginHooks =
      pluginRef &&
      typeof pluginRef === 'object' &&
      'hooks' in (pluginRef as Record<string, unknown>)
        ? parseHookBindings(cwd, (pluginRef as Record<string, unknown>).hooks)
        : undefined;
    const hooks = mergeHooks(parseHookBindings(cwd, record.hooks), pluginHooks);

    results.push({
      name,
      about: typeof record.about === 'string' ? record.about : undefined,
      alias: typeof record.alias === 'string' ? record.alias : undefined,
      aliases: stringArray(record.aliases, []),
      args: parseArgs(record.args),
      options: parseCommandOptions(record.options),
      examples: parseExamples(record.examples),
      prompt: parsePrompt(record.prompt),
      promptFlow: typeof record.promptFlow === 'string' ? record.promptFlow : undefined,
      handler: handlerSpec,
      workflow: typeof record.workflow === 'string' ? record.workflow : undefined,
      preset: typeof record.preset === 'string' ? record.preset : undefined,
      features: stringArray(record.features, []),
      dependencyRecipes: stringArray(record.dependencyRecipes, []),
      mergeRules: stringArray(record.mergeRules, []),
      guards: stringArray(record.guards, []),
      postActions: stringArray(record.postActions, []),
      hooks,
      plugin: pluginRef,
      when:
        record.when && typeof record.when === 'object'
          ? (record.when as CommandDeclaration['when'])
          : undefined,
      subcommands: parseCommands(cwd, record.subcommands),
    });
  }
  return results;
}

/**
 * 解析 manifest 中定义的 workflow 目录。
 *
 * 一个 workflow 条目既可以直接指向插件方法，也可以写成声明式步骤流。这里会保留
 * record key 作为回退名称，这样即使对象本身没有显式写 `name`，外部仍然可以稳定引用。
 */
export function parseWorkflows(
  value: unknown,
): Record<string, WorkflowDefinition> {
  const record = asRecord(value);
  const workflows: Record<string, WorkflowDefinition> = {};
  for (const [fallbackName, candidate] of Object.entries(record)) {
    const parsed = parseWorkflowEntry(fallbackName, candidate);
    if (parsed) {
      workflows[parsed.name] = parsed.workflow;
    }
  }
  return workflows;
}

/**
 * 解析可被 scaffold 命令复用的模板定义。
 *
 * 模板在这里仍然只是静态元数据，还不会执行渲染。真正的变量展开、hook 执行和依赖补丁
 * 要等到命令实际运行、拿到用户输入和环境信息以后才会发生。
 */
export function parseTemplates(value: unknown): TemplateDefinition[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const templates: TemplateDefinition[] = [];
  for (const item of value) {
    const record = asRecord(item);
    const id = typeof record.id === 'string' ? record.id.trim() : '';
    const title = typeof record.title === 'string' ? record.title.trim() : '';
    if (!id || !title) {
      continue;
    }

    templates.push({
      id,
      title,
      description: typeof record.description === 'string' ? record.description : undefined,
      tags: stringArray(record.tags, []),
      variables: Array.isArray(record.variables) ? parseTemplateVariables(record.variables) : undefined,
      hooks: record.hooks && typeof record.hooks === 'object'
        ? (record.hooks as Record<string, unknown>)
        : undefined,
    });
  }

  return templates;
}

export function parseNamedRecord<T>(value: unknown): Record<string, T> {
  const record = asRecord(value);
  const entries: Record<string, T> = {};
  for (const [key, candidate] of Object.entries(record)) {
    if (candidate && typeof candidate === 'object' && !Array.isArray(candidate)) {
      entries[key] = candidate as T;
    }
  }
  return entries;
}

export function parseManifestPlugins(value: unknown): RuntimePluginReference[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const plugins: RuntimePluginReference[] = [];
  for (const item of value) {
    if (typeof item === 'string' && item.trim()) {
      plugins.push({ package: item.trim() });
      continue;
    }

    const record = asRecord(item);
    const packageName =
      typeof record.package === 'string'
        ? record.package.trim()
        : typeof record.name === 'string'
          ? record.name.trim()
          : '';
    if (!packageName) {
      continue;
    }

    plugins.push({
      name: typeof record.name === 'string' ? record.name : undefined,
      package: packageName,
      methods: Array.isArray(record.methods)
        ? record.methods.filter((item): item is string => typeof item === 'string')
        : undefined,
      signature: typeof record.signature === 'string' ? record.signature : undefined,
      hooks: record.hooks && typeof record.hooks === 'object'
        ? (record.hooks as Record<string, unknown>)
        : undefined,
    });
  }

  return plugins;
}

/**
 * 把 manifest 命令声明编译成 bridge 两端实际消费的数据结构。
 *
 * 返回值里的 `commands` 只负责描述 CLI 形状，比如命令名、参数、选项和帮助信息；
 * `handlers` 则承载真正的执行协议，包括 hook、workflow 元数据和 scaffold 上下文。
 *
 * 这里最关键的兼容性约束是 handler id 的生成规则：它由 `mount + 命令路径 + sequence`
 * 组成，Rust host 把它当作不透明协议标识来使用，所以这里不能随意改动规则。
 */
export function buildManifestCommands(
  cwd: string,
  mount: string,
  declarations: CommandDeclaration[],
  sequence: number,
  metadata?: {
    schemaRoot?: string;
    workflows?: Record<string, WorkflowDefinition>;
    plugins?: RuntimePluginReference[];
    presets?: Record<string, import('./types.js').PresetDefinition>;
    featuresCatalog?: Record<string, import('./types.js').FeatureDefinition>;
    dependencyRecipeCatalog?: Record<string, import('./types.js').DependencyRecipeDefinition>;
    mergeRules?: Record<string, import('./types.js').MergeRuleDefinition>;
    guards?: Record<string, import('./types.js').GuardDefinition>;
    resolvers?: Record<string, import('./types.js').ResolverDefinition>;
    postActions?: Record<string, import('./types.js').PostActionDefinition>;
  },
): {
  commands: GeneratedCommandSpec[];
  handlers: DynamicHandlerRegistration[];
  nextSequence: number;
  warnings: string[];
} {
  let next = sequence;
  const handlers: DynamicHandlerRegistration[] = [];
  const warnings: string[] = [];

  const build = (path: string[], decl: CommandDeclaration): GeneratedCommandSpec => {
    const commandPath = [...path, decl.name];
    const handlerId = `dynamic.manifest.${mount}.${commandPath.join('.')}.${++next}`;
    const spec = createCommandSpec(
      decl.name,
      decl.about ?? `Dynamic command ${[mount, ...commandPath].join(' ')}`,
      handlerId,
    );
    if (decl.alias) {
      spec.alias = decl.alias;
    }
    spec.aliases = [...(decl.aliases ?? [])];
    spec.args = [...(decl.args ?? [])];
    const { options, requiredOptions } = splitRequiredOptions(decl.options ?? []);
    spec.options = options;
    spec.examples = [...(decl.examples ?? [])];

    if (decl.subcommands && decl.subcommands.length > 0) {
      // 分组节点虽然不直接执行叶子命令，也仍然需要一个 handler。
      // 这样当用户停在中间层级时，运行时仍然可以返回分组摘要和帮助视图。
      spec.subcommands = decl.subcommands.map((child) => build(commandPath, child));
      handlers.push({
        handlerId,
        method: 'command.invokeDynamic',
        plugin: 'dynamic-commands',
        target: {
          kind: 'group_summary',
          mount,
          path: commandPath,
          commands: spec.subcommands.map((command) => command.name).sort(),
        },
      });
      return spec;
    }

    // 叶子命令的执行入口既可以来自显式 handler，也可以来自命名 workflow。
    // 把 workflow 解析放在这里，能让多个命令共享一套执行定义，而不必在每条命令上重复
    // 写一遍插件元数据。
    const resolvedHandler =
      decl.handler ??
      (decl.workflow ? metadata?.workflows?.[decl.workflow] : undefined);

    if (!resolvedHandler) {
      if (decl.workflow) {
        warnings.push(
          `skip workflow command \`${[mount, ...commandPath].join(' ')}\`: workflow \`${decl.workflow}\` is not defined`,
        );
      }
      handlers.push({
        handlerId,
        method: 'command.invokeDynamic',
        plugin: 'dynamic-commands',
        target: {
          kind: 'group_summary',
          mount,
          path: commandPath,
          commands: [],
        },
      });
      return spec;
    }

    let executor: CommandExecutor;
    if (typeof resolvedHandler === 'function') {
      // 本地 JS 回调会先注册进内存表，再通过一个合成 id 暴露给 host。
      // 这样 host 调用本地函数和调用插件方法时，走的仍然是同一套协议。
      const localId = `fn:${handlerId}`;
      registerLocalExecutor(cwd, localId, resolvedHandler);
      executor = { type: 'local', id: localId };
    } else if (isDeclarativeWorkflowDefinition(resolvedHandler)) {
      const localId = `fn:${handlerId}`;
      registerLocalExecutor(cwd, localId, async (ctx) => ({
        result: await executeDeclarativeWorkflow(resolvedHandler, ctx),
        events: [],
      }));
      executor = { type: 'local', id: localId };
    } else {
      executor = { type: 'plugin', plugin: resolvedHandler.plugin, method: resolvedHandler.method };
    }

    handlers.push({
      handlerId,
      method: 'command.invokeDynamic',
      plugin: 'dynamic-commands',
      target: {
        kind: 'manifest_command',
        mount,
        path: commandPath,
        schemaRoot: metadata?.schemaRoot ?? cwd,
        declaredPlugins: metadata?.plugins ?? [],
        executor,
        prompt: decl.prompt,
        requiredOptions,
        hooks: finalizeInlineHookBindings(cwd, handlerId, decl.hooks),
          scaffold: {
            preset: decl.preset,
            features: decl.features,
            dependencyRecipes: decl.dependencyRecipes,
            mergeRules: decl.mergeRules,
            guards: decl.guards,
            postActions: decl.postActions,
            presets: metadata?.presets,
            featuresCatalog: metadata?.featuresCatalog,
            dependencyRecipeCatalog: metadata?.dependencyRecipeCatalog,
            mergeRulesCatalog: metadata?.mergeRules,
            guardsCatalog: metadata?.guards,
            resolvers: metadata?.resolvers,
            postActionsCatalog: metadata?.postActions,
          },
      },
    });
    return spec;
  };

  const commands = declarations.map((decl) => build([], decl));
  commands.sort((left, right) => left.name.localeCompare(right.name));
  return { commands, handlers, nextSequence: next, warnings };
}

/**
 * 解析命令或 workflow 条目中的“可执行部分”。
 *
 * 为了兼顾 manifest 编写体验和运行时协议统一，这里同时接受
 * `{ plugin, method }` 与 `{ type: 'plugin', plugin, method }` 两种写法，
 * 最终都会归一成同一种 handler 结构。
 */
function parseExecutable(value: unknown): CommandHandlerSpec | DynamicHandlerFn | undefined {
  if (typeof value === 'function') {
    return value as DynamicHandlerFn;
  }
  if (!value || typeof value !== 'object') {
    return undefined;
  }
  const handler = value as Record<string, unknown>;
  if (
    handler.type === 'plugin' &&
    typeof handler.plugin === 'string' &&
    typeof handler.method === 'string'
  ) {
    return { plugin: handler.plugin, method: handler.method };
  }
  if (typeof handler.plugin === 'string' && typeof handler.method === 'string') {
    return { plugin: handler.plugin, method: handler.method };
  }
  return undefined;
}

/**
 * 解析 workflow 目录中的单个条目。
 *
 * 这里支持三种形态：
 * - 直接写插件 handler
 * - 写成带 `handler` 的对象
 * - 写成带声明式 `steps` 的对象
 *
 * 这样简单 workflow 可以保持短小，多步骤流程也能写成更明确的声明式结构。
 */
function parseWorkflowEntry(
  fallbackName: string,
  value: unknown,
): { name: string; workflow: WorkflowDefinition } | undefined {
  const direct = parseExecutable(value);
  if (direct) {
    return { name: fallbackName, workflow: direct };
  }

  if (!value || typeof value !== 'object') {
    return undefined;
  }

  const record = value as Record<string, unknown>;
  const name =
    typeof record.name === 'string' && record.name.trim() ? record.name.trim() : fallbackName;
  const declarative = parseDeclarativeWorkflow(record);
  if (declarative) {
    return { name, workflow: { ...declarative, name } };
  }
  const handler = parseExecutable(record.handler);
  if (!handler) {
    return undefined;
  }

  return { name, workflow: handler };
}

/**
 * 解析声明式 workflow 定义。
 *
 * 每个 step 可以是字符串简写，也可以是带 `name + options` 的对象。无效 step 会被静默
 * 丢弃，避免单个坏条目导致整个 manifest 加载失败。
 */
function parseDeclarativeWorkflow(
  value: Record<string, unknown>,
): DeclarativeWorkflowDefinition | undefined {
  if (!Array.isArray(value.steps)) {
    return undefined;
  }
  const steps = value.steps
    .map((step) => {
      if (typeof step === 'string' && step.trim()) {
        return step.trim() as DeclarativeWorkflowDefinition['steps'][number];
      }
      if (!step || typeof step !== 'object' || Array.isArray(step)) {
        return null;
      }
      const record = step as Record<string, unknown>;
      const name = typeof record.name === 'string' ? record.name.trim() : '';
      if (!name) {
        return null;
      }
      return {
        name: name as Extract<DeclarativeWorkflowDefinition['steps'][number], { name: string }>['name'],
        options:
          record.options && typeof record.options === 'object' && !Array.isArray(record.options)
            ? (record.options as Record<string, unknown>)
            : undefined,
      };
    })
    .filter((step): step is DeclarativeWorkflowDefinition['steps'][number] => Boolean(step));
  if (steps.length === 0) {
    return undefined;
  }
  return {
    guards: stringArray(value.guards, []),
    steps,
  };
}

function isDeclarativeWorkflowDefinition(
  value: WorkflowDefinition,
): value is DeclarativeWorkflowDefinition {
  return Boolean(value && typeof value === 'object' && 'steps' in value);
}

function parseTemplateVariable(
  value: unknown,
): TemplateVariableDefinition | undefined {
  const record = asRecord(value);
  const key = typeof record.key === 'string' ? record.key.trim() : '';
  if (!key) {
    return undefined;
  }

  return {
    key,
    type: typeof record.type === 'string' ? record.type : undefined,
    required: record.required === true,
    default: record.default,
    description: typeof record.description === 'string' ? record.description : undefined,
    choices: Array.isArray(record.choices)
      ? record.choices.filter(
          (choice): choice is string | number =>
            typeof choice === 'string' || typeof choice === 'number',
        )
      : undefined,
  };
}

function parseTemplateVariables(values: unknown[]): TemplateVariableDefinition[] {
  const variables: TemplateVariableDefinition[] = [];
  for (const value of values) {
    const variable = parseTemplateVariable(value);
    if (variable) {
      variables.push(variable);
    }
  }
  return variables;
}

/**
 * 把命令选项声明归一成 Rust bridge 侧期望的 schema 结构。
 *
 * 这里本质上是一个兼容层：manifest 作者可以写 `defaultValue` 这种更顺手的字段名，
 * 最终会被转换成 command spec 使用的 snake_case 协议字段。
 */
function parseCommandOptions(
  value: unknown,
): Array<GeneratedCommandSpec['options'][number] & { required?: boolean }> {
  if (!Array.isArray(value)) {
    return [];
  }

  const results: Array<GeneratedCommandSpec['options'][number] & { required?: boolean }> = [];
  for (const item of value) {
    const option = asRecord(item);
    const long = typeof option.long === 'string' ? sanitizeSegment(option.long) : '';
    if (!long) {
      continue;
    }

    const rawShort = option.short;
    const short = typeof rawShort === 'string' && rawShort.length > 0 ? rawShort[0] : null;
    const valueKind = normalizeValueKind(option.valueKind ?? option.value_kind);
    results.push({
      long,
      short,
      help: typeof option.help === 'string' ? option.help : '',
      value_kind: valueKind,
      default_value:
        option.defaultValue !== undefined && option.defaultValue !== null
          ? String(option.defaultValue)
          : option.default_value !== undefined && option.default_value !== null
            ? String(option.default_value)
            : null,
      choices: Array.isArray(option.choices)
        ? option.choices
            .filter(
              (choice): choice is string | number =>
                typeof choice === 'string' || typeof choice === 'number',
            )
            .map(String)
        : [],
      negatable: option.negatable === true,
      required: option.required === true,
    });
  }
  return results;
}

function parseArgs(value: unknown): GeneratedCommandSpec['args'] {
  return Array.isArray(value)
    ? value
        .map((item) => {
          const arg = asRecord(item);
          return {
            name: typeof arg.name === 'string' ? arg.name : '',
            required: arg.required === true,
            multiple: arg.multiple === true,
            help: typeof arg.help === 'string' ? arg.help : '',
          };
        })
        .filter((arg) => arg.name)
    : [];
}

/**
 * 把作者侧使用的 `required` 标记从 option 对象中拆出来，转成单独的必填选项列表。
 *
 * host 协议不会把“是否必填”直接内嵌在 option 结构里，所以这里要做一次拆分，既保持
 * CLI schema 简洁，也不丢失运行时校验所需的信息。
 */
function splitRequiredOptions(
  options: Array<GeneratedCommandSpec['options'][number] & { required?: boolean }>,
): { options: GeneratedCommandSpec['options']; requiredOptions: string[] } {
  const requiredOptions: string[] = [];
  const normalized = options.map((option) => {
    if (option.required) {
      requiredOptions.push(option.long);
    }
    const { required: _required, ...rest } = option;
    return rest;
  });
  return { options: normalized, requiredOptions };
}
