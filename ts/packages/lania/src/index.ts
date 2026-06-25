import type {
  ConfigEnv,
  NamedWorkflowDefinition,
  PluginDefinition,
  ProductConfig,
  ProductDefinition,
  SchemasConfig,
  TemplateDefinition,
  WorkflowDefinition,
} from './types.js';

export type { SchemaTools } from '@lania-cli/node-bridge';
export type * from './types.js';

export function defineConfig<T extends ProductConfig>(config: T): T;
export function defineConfig<T extends ProductConfig>(factory: () => T): () => T;
export function defineConfig<T extends ProductConfig>(
  factory: (env: ConfigEnv) => T | Promise<T>,
): (env: ConfigEnv) => T | Promise<T>;
export function defineConfig<T extends ProductConfig>(
  value: T | (() => T) | ((env: ConfigEnv) => T | Promise<T>),
): T | (() => T) | ((env: ConfigEnv) => T | Promise<T>) {
  return value;
}

export function defineSchemas<T extends SchemasConfig>(config: T): T;
export function defineSchemas<T extends SchemasConfig>(factory: () => T): () => T;
export function defineSchemas<T extends SchemasConfig>(value: T | (() => T)): T | (() => T) {
  return value;
}

export function defineProduct<T extends ProductDefinition>(product: T): T {
  return product;
}

export function defineTemplate<T extends TemplateDefinition>(template: T): T;
export function defineTemplate<T extends TemplateDefinition[]>(templates: T): T;
export function defineTemplate<T extends TemplateDefinition | TemplateDefinition[]>(value: T): T {
  return value;
}

export function definePlugin<T extends PluginDefinition>(plugin: T): T {
  return plugin;
}

export function defineWorkflow<T extends WorkflowDefinition>(workflow: T): T;
export function defineWorkflow<TName extends string, T extends WorkflowDefinition>(
  name: TName,
  workflow: T,
): NamedWorkflowDefinition<TName>;
export function defineWorkflow<T extends WorkflowDefinition, TName extends string>(
  nameOrWorkflow: TName | T,
  workflow?: T,
): T | NamedWorkflowDefinition<TName> {
  if (typeof nameOrWorkflow === 'string') {
    return {
      name: nameOrWorkflow,
      handler: workflow as T,
    };
  }
  return nameOrWorkflow;
}
