import type { BridgeEvent } from '../../protocol/events.js';
import { loadDeclaredTemplates } from './discovery.js';
import {
  loadTemplatesRuntime,
  renderAddTemplateName,
  renderTemplateName,
  resolveTemplate,
} from './runtime.js';
import type { RuntimeInput } from './types.js';

// service 层负责把 runtime 与 discovery 两类能力编排成 bridge 可直接返回的结果。
// 它不负责加载来源判断，也不负责清单发现细节，只负责“组合”和“整形”。
export async function listResult(runtimeInput: RuntimeInput) {
  const runtime = await loadTemplatesRuntime();
  const templates = await runtime.listRuntimeTemplates(runtimeInput);
  const declaredTemplates = await loadDeclaredTemplates(runtimeInput.cwd);
  const declaredTemplateMap = new Map(declaredTemplates.map((template) => [template.id, template]));
  // runtime 返回的是模板运行时视角的 manifest 信息；
  // declared templates 补充的是产品/schema 声明里的标题、描述、标签等元数据。
  // 这里把两者合并，形成对外更完整的模板列表视图。
  return {
    result: {
      templates: templates.map((template) => template.manifest.name),
      metadata: templates.map((template) => ({
        name: template.manifest.name,
        title: declaredTemplateMap.get(template.manifest.name)?.title ?? template.manifest.name,
        description: declaredTemplateMap.get(template.manifest.name)?.description ?? null,
        tags: declaredTemplateMap.get(template.manifest.name)?.tags ?? [],
        schemaVersion: template.manifest.schemaVersion,
        renderEngine: template.manifest.renderEngine,
        legacyTemplateDir: template.manifest.legacyTemplateDir,
        ownership: template.manifest.ownership,
        useCases: template.manifest.useCases,
        migrationLayer: template.manifest.migrationLayer ?? 'unknown',
      })),
    },
    events: [] as BridgeEvent[],
  };
}

// questions/dependencies/outputTasks 都遵循同一模式：
// 1. 先把请求模板名解析为真实模板
// 2. 再调用 runtime 对应能力
// 这样模板回退与运行时加载逻辑不会散落在多个 handler 中。
export async function questionsResult(
  templateName: unknown,
  runtimeInput: RuntimeInput,
) {
  const template = await resolveTemplate(templateName, runtimeInput);
  const runtime = await loadTemplatesRuntime();
  const questions = await runtime.loadTemplateQuestions(template.manifest.name, runtimeInput);
  return {
    result: {
      template: template.manifest.name,
      questions,
    },
    events: [] as BridgeEvent[],
  };
}

export async function dependenciesResult(
  templateName: unknown,
  runtimeInput: RuntimeInput,
) {
  const template = await resolveTemplate(templateName, runtimeInput);
  const runtime = await loadTemplatesRuntime();
  const dependencies = await runtime.loadTemplateDependencies(template.manifest.name, runtimeInput);
  return {
    result: {
      template: template.manifest.name,
      dependencies: dependencies.dependencies,
      devDependencies: dependencies.devDependencies,
    },
    events: [] as BridgeEvent[],
  };
}

export async function outputTasksResult(
  templateName: unknown,
  runtimeInput: RuntimeInput,
) {
  const template = await resolveTemplate(templateName, runtimeInput);
  const runtime = await loadTemplatesRuntime();
  return {
    result: {
      template: template.manifest.name,
      tasks: await runtime.loadTemplateOutputTasks(template.manifest.name, runtimeInput),
    },
    events: [] as BridgeEvent[],
  };
}

export async function renderResult(
  templateName: unknown,
  runtimeInput: RuntimeInput,
) {
  const runtime = await loadTemplatesRuntime();
  // render 直接透传给 runtime，是因为渲染结果本身通常已经带有完整事件和数据结构，
  // 这里不再额外包一层，避免破坏模板运行时的原始输出语义。
  return runtime.renderRuntimeTemplate(renderTemplateName(templateName), runtimeInput);
}

export async function renderAddTemplateResult(
  templateName: unknown,
  context: Record<string, unknown>,
) {
  const runtime = await loadTemplatesRuntime();
  return runtime.renderAddRuntimeTemplate(renderAddTemplateName(templateName), { context });
}

// 统一构造模板相关的日志事件，避免各处手写 event.log 结构。
export function renderLogEvent(message: string): BridgeEvent {
  return {
    method: 'event.log',
    params: {
      level: 'info',
      message,
    },
  };
}
