/**
 * 为运行时暴露 `tools.scaffold` 能力。
 *
 * 它处在“声明式 scaffold plan”和“真正的模板渲染 / 依赖安装动作”之间：
 * - 上游传进来的是已经解析好的 `ScaffoldPlan`
 * - 这里把 plan 转成模板渲染结果和依赖安装计划
 * - 但不会直接写文件或安装依赖，真正执行动作留给更外层流程控制
 */
import { templatePlugin } from '../plugins/template.js';
import type {
  ProductContext,
  RuntimeContext,
  ScaffoldPlan,
} from '../plugins/dynamic-commands/types.js';
import type { PackageManagerTools } from './pm.js';
import type { SchemaToolContext } from './types.js';

export interface ScaffoldTemplateFile {
  path: string;
  content: string;
}

export interface ScaffoldRenderResult {
  templates: string[];
  files: ScaffoldTemplateFile[];
  collisions: string[];
}

export interface ScaffoldDependencyPlanResult {
  manager: string;
  templates: string[];
  dependencies: string[];
  devDependencies: string[];
  scripts: Record<string, string>;
  packageJsonPatch: {
    dependencies: Record<string, string>;
    devDependencies: Record<string, string>;
    scripts: Record<string, string>;
  };
  installCommands: Array<{ program: string; args: string[] }>;
}

export interface ScaffoldTools {
  currentPlan: () => ScaffoldPlan;
  renderTemplateLayers: (input?: {
    layers?: string[];
    context?: Record<string, unknown>;
    options?: Record<string, unknown>;
  }) => Promise<ScaffoldRenderResult>;
  dependencyPlan: (input?: {
    layers?: string[];
    manager?: string;
    includeTemplateDependencies?: boolean;
    context?: Record<string, unknown>;
    options?: Record<string, unknown>;
  }) => Promise<ScaffoldDependencyPlanResult>;
}

interface CreateScaffoldToolsParams {
  base: SchemaToolContext;
  pm: PackageManagerTools;
  scaffold?: ScaffoldPlan;
  runtime?: RuntimeContext;
  product?: ProductContext;
}

export function createScaffoldTools(params: CreateScaffoldToolsParams): ScaffoldTools {
  const scaffold = params.scaffold ?? emptyScaffoldPlan();

  return {
    currentPlan: () => cloneScaffoldPlan(scaffold),
    renderTemplateLayers: async (input) => {
      // 渲染模板层时遵循“后渲染覆盖先渲染”的规则。
      // 如果多个模板层产出了同一路径，最终文件内容以后者为准，同时把冲突路径记录下来，
      // 交给上层决定是否提示用户。
      const templateNames = resolveTemplateLayers(scaffold, input?.layers);
      const collisions: string[] = [];
      const filesByPath = new Map<string, ScaffoldTemplateFile>();

      for (const template of templateNames) {
        const rendered = await templatePlugin.handle('template.render', {
          cwd: resolveTemplateCwd(params),
          template,
          context: buildTemplateContext(params, scaffold, input?.context),
          options: buildTemplateOptions(scaffold, input?.options),
        });
        const result = rendered?.result as
          | {
              files?: Array<{ path?: string; content?: string }>;
            }
          | undefined;

        for (const file of result?.files ?? []) {
          if (typeof file.path !== 'string' || typeof file.content !== 'string') {
            continue;
          }
          if (filesByPath.has(file.path)) {
            collisions.push(file.path);
          }
          filesByPath.set(file.path, {
            path: file.path,
            content: file.content,
          });
        }
      }

      return {
        templates: templateNames,
        files: [...filesByPath.values()],
        collisions: uniqueStrings(collisions),
      };
    },
    dependencyPlan: async (input) => {
      // 依赖计划来自两部分：
      // 1. scaffold plan 里已经显式声明的依赖
      // 2. 模板自身通过 `template.getDependencies` 报告的依赖
      //
      // 这里仅负责汇总、去重和生成安装命令，不直接执行安装。
      const templateNames = resolveTemplateLayers(scaffold, input?.layers);
      const dependencies = [...scaffold.dependencies];
      const devDependencies = [...scaffold.devDependencies];
      const scripts = { ...scaffold.scripts };

      if (input?.includeTemplateDependencies !== false) {
        for (const template of templateNames) {
          const handled = await templatePlugin.handle('template.getDependencies', {
            cwd: resolveTemplateCwd(params),
            template,
            context: buildTemplateContext(params, scaffold, input?.context),
            options: buildTemplateOptions(scaffold, input?.options),
          });
          const result = handled?.result as
            | {
                dependencies?: string[];
                devDependencies?: string[];
              }
            | undefined;
          dependencies.push(...(result?.dependencies ?? []));
          devDependencies.push(...(result?.devDependencies ?? []));
        }
      }

      const manager = params.pm.binary(input?.manager ?? scaffold.packageManager ?? 'npm');
      const resolvedDependencies = uniqueStrings(dependencies);
      const resolvedDevDependencies = uniqueStrings(devDependencies);
      const installCommands = await params.pm.command.addDependencyCommands({
        manager,
        dependencies: resolvedDependencies,
        devDependencies: resolvedDevDependencies,
      });

      return {
        manager,
        templates: templateNames,
        dependencies: resolvedDependencies,
        devDependencies: resolvedDevDependencies,
        scripts,
        packageJsonPatch: {
          dependencies: toDependencyRecord(resolvedDependencies),
          devDependencies: toDependencyRecord(resolvedDevDependencies),
          scripts,
        },
        installCommands,
      };
    },
  };
}

function resolveTemplateLayers(scaffold: ScaffoldPlan, layers?: string[]): string[] {
  // 允许调用方临时覆写要渲染的 layer 集合；未覆写时使用 scaffold plan 中已经解析好的层。
  return uniqueStrings(layers ?? scaffold.templateLayers);
}

function resolveTemplateCwd(params: CreateScaffoldToolsParams): string {
  // 模板 cwd 优先贴近 runtime/product 的真实根目录，避免在嵌套执行场景里错误落回当前 shell cwd。
  return params.runtime?.productRoot ?? params.product?.productRoot ?? params.base.cwd;
}

function buildTemplateContext(
  params: CreateScaffoldToolsParams,
  scaffold: ScaffoldPlan,
  context?: Record<string, unknown>,
): Record<string, unknown> {
  // 模板上下文同时暴露 scaffold、runtime、product 和若干常用路径，
  // 这样模板无需自己重复推断“当前工作区根在哪、产品根在哪、包管理器是什么”。
  return {
    ...(context ?? {}),
    scaffold,
    runtime: params.runtime ?? null,
    product: params.product ?? null,
    workspaceRoot: params.runtime?.workspaceRoot ?? params.base.cwd,
    productRoot: params.runtime?.productRoot ?? params.product?.productRoot ?? params.base.cwd,
    packageManager: scaffold.packageManager ?? undefined,
  };
}

function buildTemplateOptions(
  scaffold: ScaffoldPlan,
  options?: Record<string, unknown>,
): Record<string, unknown> {
  // 选项层比 context 更偏“渲染控制参数”。
  // 这里会把已经解析好的依赖结果也塞进去，方便模板直接消费。
  return {
    ...(options ?? {}),
    packageManager: options?.packageManager ?? scaffold.packageManager ?? undefined,
    resolvedDependencies: options?.resolvedDependencies ?? toDependencyRecord(scaffold.dependencies),
    resolvedDevDependencies:
      options?.resolvedDevDependencies ?? toDependencyRecord(scaffold.devDependencies),
  };
}

function cloneScaffoldPlan(scaffold: ScaffoldPlan): ScaffoldPlan {
  // 向调用方暴露 plan 时返回拷贝，避免外部误改内部缓存。
  return {
    preset: scaffold.preset,
    features: [...scaffold.features],
    templateLayers: [...scaffold.templateLayers],
    dependencyRecipes: [...scaffold.dependencyRecipes],
    dependencies: [...scaffold.dependencies],
    devDependencies: [...scaffold.devDependencies],
    scripts: { ...scaffold.scripts },
    packageManager: scaffold.packageManager,
    resolvers: Object.fromEntries(
      Object.entries(scaffold.resolvers).map(([name, resolver]) => [name, { ...resolver }]),
    ),
    mergeRules: scaffold.mergeRules.map((rule) => ({ ...rule })),
    guards: scaffold.guards.map((guard) => ({ ...guard })),
    postActions: scaffold.postActions.map((action) => ({ ...action })),
  };
}

function emptyScaffoldPlan(): ScaffoldPlan {
  // 没有传入 scaffold 时，工具层仍然返回一个语义完整的空计划，减少上层判空分支。
  return {
    preset: null,
    features: [],
    templateLayers: [],
    dependencyRecipes: [],
    dependencies: [],
    devDependencies: [],
    scripts: {},
    packageManager: null,
    resolvers: {},
    mergeRules: [],
    guards: [],
    postActions: [],
  };
}

function uniqueStrings(values: string[]): string[] {
  // 按首次出现顺序去重，保持模板层和依赖输出的稳定性。
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

function toDependencyRecord(entries: string[]): Record<string, string> {
  // 把 `pkg@version` 形式的条目转换成 package.json 兼容的 record。
  // 没带版本时统一回退到 `latest`，与脚手架阶段“先生成计划、后再精修版本”的策略保持一致。
  return Object.fromEntries(
    entries.map((entry) => {
      const versionSeparator = entry.lastIndexOf('@');
      if (versionSeparator > 0) {
        return [entry.slice(0, versionSeparator), entry.slice(versionSeparator + 1)];
      }
      return [entry, 'latest'];
    }),
  );
}
