import type { TemplateDefinition as DeclaredTemplateDefinition } from '../dynamic-commands/types.js';

// `template` 插件拆分后，这个文件只保留跨子模块共享的最小类型定义。
// 目的不是集中业务逻辑，而是避免 runtime/discovery/service 互相从实现文件里反向引用类型，
// 从而保持依赖方向单一。
export type RuntimeInput = {
  cwd: string | null;
  context: Record<string, unknown>;
  options: Record<string, unknown>;
};

// 模板运行时既可能来自工作区源码，也可能来自已发布的 npm 包。
// 因此这里抽象出一组统一接口，让上层只依赖“模板运行时能力”，
// 不依赖具体加载来源。
export type TemplatesRuntimeModule = {
  getRuntimeTemplate: (template: string, input: RuntimeInput) => Promise<any>;
  listRuntimeTemplates: (input: RuntimeInput) => Promise<any[]>;
  loadTemplateDependencies: (template: string, input: RuntimeInput) => Promise<any>;
  loadTemplateOutputTasks: (template: string, input: RuntimeInput) => Promise<string[]>;
  loadTemplateQuestions: (template: string, input: RuntimeInput) => Promise<any[]>;
  renderRuntimeTemplate: (template: string, input: RuntimeInput) => Promise<any>;
  renderAddRuntimeTemplate: (
    template: string,
    input: { context: Record<string, unknown> },
  ) => Promise<any>;
};

export type { DeclaredTemplateDefinition };
