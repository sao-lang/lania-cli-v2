import { access } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import type { RuntimeInput, TemplatesRuntimeModule } from './types.js';

let templatesRuntimePromise: Promise<TemplatesRuntimeModule> | null = null;

// 负责把 bridge 请求里的弱类型参数收敛为模板运行时可消费的稳定输入结构。
// 这一层只做形状归一化，不做模板发现或业务分发。
export function normalizeRuntimeInput(params: Record<string, unknown>): RuntimeInput {
  return {
    cwd: typeof params.cwd === 'string' ? params.cwd : null,
    context: asObject(params.context),
    options: asObject(params.options),
  };
}

// 模板名解析集中放在这里，目的是统一默认模板回退策略。
// 当前约定是：如果请求模板不存在，则回退到 `spa-react`，保证旧调用方仍能得到可用结果。
export async function resolveTemplate(
  templateName: unknown,
  runtimeInput: RuntimeInput,
) {
  const requested = asTemplateName(templateName);
  const runtime = await loadTemplatesRuntime();
  return (
    (await runtime.getRuntimeTemplate(requested, runtimeInput)) ??
    (await runtime.getRuntimeTemplate('spa-react', runtimeInput))!
  );
}

// 模板运行时的加载代价相对较高，因此这里做进程级缓存。
// 拆分后 service 层只关心“取到 runtime 并调用”，不需要重复处理加载与缓存逻辑。
export async function loadTemplatesRuntime(): Promise<TemplatesRuntimeModule> {
  if (!templatesRuntimePromise) {
    templatesRuntimePromise = resolveTemplatesRuntime();
  }
  return templatesRuntimePromise;
}

export function renderTemplateName(value: unknown): string {
  return asTemplateName(value);
}

export function renderAddTemplateName(value: unknown): string {
  return typeof value === 'string' && value.length > 0 ? value : 'rfc';
}

function asObject(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
}

function asTemplateName(value: unknown): string {
  return typeof value === 'string' && value.length > 0 ? value : 'spa-react';
}

// 开发态优先尝试直接加载工作区 `templates` 源码，方便联调；
// 如果当前环境拿不到源码入口，再退回到发布包 `@lania-cli/templates`。
// 这样同一套 service/discovery 逻辑可以同时服务源码运行与安装运行。
async function resolveTemplatesRuntime(): Promise<TemplatesRuntimeModule> {
  const pluginDir = dirname(fileURLToPath(import.meta.url));
  const workspaceEntry = resolve(pluginDir, '../../../../templates/src/index.ts');
  if (await fileExists(workspaceEntry)) {
    try {
      return (await import(pathToFileURL(workspaceEntry).href)) as TemplatesRuntimeModule;
    } catch {
      // fall through to the published package when .ts source import is unavailable.
    }
  }
  return (await import('@lania-cli/templates')) as TemplatesRuntimeModule;
}

// 这里只做最轻量的文件存在性探测，用于决定运行时加载来源。
// 失败直接视为不存在，避免把探测错误暴露给上层业务逻辑。
async function fileExists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}
