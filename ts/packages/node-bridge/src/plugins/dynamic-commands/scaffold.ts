/**
 * 负责把命令上的 scaffold 声明整理成真正可执行的 scaffold plan。
 *
 * 这个模块的核心职责不是渲染模板，而是做“计划编排”：
 * - 根据 args / options 计算条件表达式是否生效
 * - 叠加 preset、feature、recipe 带来的模板层和依赖
 * - 产出 merge rule、guard、resolver、post action 等后续阶段要消费的结构
 */
import type {
  ConditionSchema,
  DependencyRecipeDefinition,
  GuardDefinition,
  ManifestCommandScaffoldSpec,
  MergeRuleDefinition,
  PostActionDefinition,
  ScaffoldGuardPlan,
  ScaffoldMergeRulePlan,
  ScaffoldPostActionPlan,
  ResolverDefinition,
  ScaffoldPlan,
  ScaffoldResolverPlan,
} from './types.js';

interface ResolveScaffoldContextParams {
  args: Record<string, unknown>;
  options: Record<string, unknown>;
  spec?: ManifestCommandScaffoldSpec;
}

interface EvaluationScope {
  args: Record<string, unknown>;
  options: Record<string, unknown>;
  argv: {
    args: Record<string, unknown>;
    options: Record<string, unknown>;
  };
  preset: string | null;
  features: string[];
}

export function resolveScaffoldPlan(params: ResolveScaffoldContextParams): ScaffoldPlan {
  // 这里定义了 scaffold 叠加的主顺序：
  // 1. 先确定 preset 是否生效
  // 2. 再展开 preset 和显式声明带来的 features
  // 3. 再从 feature/preset 汇总 template layer、dependency recipe、merge rule 等能力
  //
  // 整个过程会持续做去重，确保最终 plan 稳定且顺序可预测。
  const spec = params.spec;
  const evaluationScope = createEvaluationScope(params.args, params.options, spec);
  const presetName = resolvePresetName(spec, evaluationScope);
  const preset = presetName ? spec?.presets?.[presetName] : undefined;

  const featureNames = uniqueStrings([
    ...(preset?.features ?? []).filter((name) =>
      isConditionActive(spec?.featuresCatalog?.[name]?.when, evaluationScope),
    ),
    ...(spec?.features ?? []).filter((name) =>
      isConditionActive(spec?.featuresCatalog?.[name]?.when, evaluationScope),
    ),
  ]);

  const templateLayers = uniqueStrings([
    ...(preset?.templateLayers ?? []),
    ...featureNames.flatMap((name) => spec?.featuresCatalog?.[name]?.templateLayers ?? []),
  ]);

  const dependencyRecipeNames = uniqueStrings([
    ...(preset?.dependencyRecipes ?? []).filter((name) =>
      isConditionActive(spec?.dependencyRecipeCatalog?.[name]?.when, evaluationScope),
    ),
    ...featureNames.flatMap((name) =>
      (spec?.featuresCatalog?.[name]?.dependencyRecipes ?? []).filter((recipeName) =>
        isConditionActive(spec?.dependencyRecipeCatalog?.[recipeName]?.when, evaluationScope),
      ),
    ),
    ...(spec?.dependencyRecipes ?? []).filter((name) =>
      isConditionActive(spec?.dependencyRecipeCatalog?.[name]?.when, evaluationScope),
    ),
  ]);

  const recipePlans = dependencyRecipeNames
    .map((name) => spec?.dependencyRecipeCatalog?.[name])
    .filter((recipe): recipe is DependencyRecipeDefinition => Boolean(recipe));

  const scripts: Record<string, string> = {};
  for (const recipe of recipePlans) {
    Object.assign(scripts, recipe.scripts ?? {});
  }

  const mergeRules = uniqueMergeRules(
    uniqueStrings([
      ...(preset?.mergeRules ?? []).filter((name) =>
        isConditionActive(spec?.mergeRulesCatalog?.[name]?.when, evaluationScope),
      ),
      ...featureNames.flatMap((name) =>
        (spec?.featuresCatalog?.[name]?.mergeRules ?? []).filter((ruleName) =>
          isConditionActive(spec?.mergeRulesCatalog?.[ruleName]?.when, evaluationScope),
        ),
      ),
      ...(spec?.mergeRules ?? []).filter((name) =>
        isConditionActive(spec?.mergeRulesCatalog?.[name]?.when, evaluationScope),
      ),
    ])
      .map((name) =>
        toScaffoldMergeRulePlan(name, spec?.mergeRulesCatalog?.[name], evaluationScope),
      )
      .filter((rule): rule is ScaffoldMergeRulePlan => Boolean(rule)),
  );

  const guards = uniqueGuards(
    uniqueStrings([
      ...(preset?.guards ?? []).filter((name) =>
        isConditionActive(spec?.guardsCatalog?.[name]?.when, evaluationScope),
      ),
      ...(spec?.guards ?? []).filter((name) =>
        isConditionActive(spec?.guardsCatalog?.[name]?.when, evaluationScope),
      ),
    ])
      .map((name) => toScaffoldGuardPlan(name, spec?.guardsCatalog?.[name], evaluationScope))
      .filter((guard): guard is ScaffoldGuardPlan => Boolean(guard)),
  );

  const resolvers = Object.fromEntries(
    Object.entries(spec?.resolvers ?? {})
      .filter(([, resolver]) => isConditionActive(resolver.when, evaluationScope))
      .map(([name, resolver]) => [name, toScaffoldResolverPlan(resolver)]),
  ) as Record<string, ScaffoldResolverPlan>;
  const postActions = uniquePostActions(
    [...(preset?.postActions ?? []), ...(spec?.postActions ?? [])]
      .map((name) =>
        toScaffoldPostActionPlan(name, spec?.postActionsCatalog?.[name], evaluationScope),
      )
      .filter((action): action is ScaffoldPostActionPlan => Boolean(action)),
  );

  return {
    preset: presetName,
    features: featureNames,
    templateLayers,
    dependencyRecipes: dependencyRecipeNames,
    dependencies: uniqueStrings(recipePlans.flatMap((recipe) => recipe.dependencies ?? [])),
    devDependencies: uniqueStrings(recipePlans.flatMap((recipe) => recipe.devDependencies ?? [])),
    scripts,
    packageManager:
      [...recipePlans].reverse().find((recipe) => typeof recipe.packageManager === 'string')
        ?.packageManager ?? null,
    resolvers,
    mergeRules,
    guards,
    postActions,
  };
}

function createEvaluationScope(
  args: Record<string, unknown>,
  options: Record<string, unknown>,
  spec?: ManifestCommandScaffoldSpec,
): EvaluationScope {
  // 把条件求值需要访问的数据统一摊平成一个 scope，避免每种条件都手写取值分支。
  return {
    args,
    options,
    argv: { args, options },
    preset: spec?.preset ?? null,
    features: spec?.features ?? [],
  };
}

function resolvePresetName(
  spec: ManifestCommandScaffoldSpec | undefined,
  scope: EvaluationScope,
): string | null {
  // preset 本身也允许挂条件；条件不满足时，视为“没有选中 preset”，而不是报错。
  if (!spec?.preset) {
    return null;
  }
  const preset = spec.presets?.[spec.preset];
  if (!preset) {
    return null;
  }
  return isConditionActive(preset.when, scope) ? spec.preset : null;
}

function isConditionActive(
  condition: ConditionSchema | undefined,
  scope: EvaluationScope,
): boolean {
  // 条件表达式支持三类能力：
  // - `all` / `any`：组合条件
  // - `equals` / `in`：值比较
  // - `truthy`：布尔存在性判断
  //
  // 未声明条件时默认生效，这是为了让 manifest 作者只在需要限制时才写 `when`。
  if (!condition) {
    return true;
  }
  if ('all' in condition) {
    return condition.all.every((entry) => isConditionActive(entry, scope));
  }
  if ('any' in condition) {
    return condition.any.some((entry) => isConditionActive(entry, scope));
  }
  const fieldValue = readField(scope, condition.field);
  if ('equals' in condition) {
    return fieldValue === condition.equals;
  }
  if ('in' in condition) {
    return condition.in.includes(fieldValue);
  }
  if ('truthy' in condition) {
    return Boolean(fieldValue);
  }
  return true;
}

function readField(scope: EvaluationScope, field: string): unknown {
  // 字段读取同时支持：
  // - 简写字段名：优先从 options，再回退到 args，再回退到 scope 顶层
  // - 点路径：例如 `argv.options.template`
  //
  // 这种规则既兼容简洁写法，也允许条件表达式显式指定读取来源。
  if (!field.includes('.')) {
    if (field in scope.options) {
      return scope.options[field];
    }
    if (field in scope.args) {
      return scope.args[field];
    }
    return (scope as unknown as Record<string, unknown>)[field];
  }

  const segments = field.split('.');
  let current: unknown = scope;
  for (const segment of segments) {
    if (!current || typeof current !== 'object') {
      return undefined;
    }
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function toScaffoldResolverPlan(resolver: ResolverDefinition): ScaffoldResolverPlan {
  return {
    from: resolver.from,
    use: resolver.use,
  };
}

function toScaffoldMergeRulePlan(
  name: string,
  rule: MergeRuleDefinition | undefined,
  scope: EvaluationScope,
): ScaffoldMergeRulePlan | null {
  // merge rule 只有在定义存在且条件命中时才会进入最终 plan。
  if (!rule || !isConditionActive(rule.when, scope)) {
    return null;
  }
  return {
    name,
    target: rule.target,
    strategy: rule.strategy,
  };
}

function toScaffoldGuardPlan(
  name: string,
  guard: GuardDefinition | undefined,
  scope: EvaluationScope,
): ScaffoldGuardPlan | null {
  // 这里把声明式 guard 配置压成运行时更容易执行的 plan 结构。
  // 未识别的 guard 类型会被忽略，避免旧 manifest 因新类型未实现而整体失效。
  if (!guard || !isConditionActive(guard.when, scope)) {
    return null;
  }
  switch (guard.type) {
    case 'directory_empty':
      return { name, type: guard.type };
    case 'node_version':
      return { name, type: guard.type, range: guard.range };
    case 'command_exists':
      return { name, type: guard.type, command: guard.command };
    case 'workspace_kind':
      return { name, type: guard.type, value: guard.value };
    default:
      return null;
  }
}

function toScaffoldPostActionPlan(
  name: string,
  action: PostActionDefinition | undefined,
  scope: EvaluationScope,
): ScaffoldPostActionPlan | null {
  // post action 在这里仅保留执行时真正需要的字段，避免把整个定义对象泄漏到运行时计划里。
  if (!action || !isConditionActive(action.when, scope)) {
    return null;
  }
  return {
    name,
    type: action.type,
    ...(typeof action.message === 'string' ? { message: action.message } : {}),
  };
}

function uniqueStrings(values: string[]): string[] {
  // 按首次出现顺序去重，保证叠加多个来源后的 plan 仍然稳定可预测。
  const result: string[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value || seen.has(value)) {
      continue;
    }
    seen.add(value);
    result.push(value);
  }
  return result;
}

function uniquePostActions(values: ScaffoldPostActionPlan[]): ScaffoldPostActionPlan[] {
  const result: ScaffoldPostActionPlan[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value.name || seen.has(value.name)) {
      continue;
    }
    seen.add(value.name);
    result.push(value);
  }
  return result;
}

function uniqueMergeRules(values: ScaffoldMergeRulePlan[]): ScaffoldMergeRulePlan[] {
  const result: ScaffoldMergeRulePlan[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value.name || seen.has(value.name)) {
      continue;
    }
    seen.add(value.name);
    result.push(value);
  }
  return result;
}

function uniqueGuards(values: ScaffoldGuardPlan[]): ScaffoldGuardPlan[] {
  const result: ScaffoldGuardPlan[] = [];
  const seen = new Set<string>();
  for (const value of values) {
    if (!value.name || seen.has(value.name)) {
      continue;
    }
    seen.add(value.name);
    result.push(value);
  }
  return result;
}
