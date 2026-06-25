import { readdir } from 'node:fs/promises';
import { extname, resolve } from 'node:path';

import type { BridgeEvent } from '../../protocol/events.js';
import { createDefaultPluginSecurityPolicy } from '../../core/plugin-policy.js';
import { asRecord, fileExists, loadConfigModule, loadLanConfig } from '../../core/runtime.js';
import { loadCommandPlugin, mergePluginDeclarations } from './plugin-runtime.js';
import { parseCommandPackMeta, stringArray } from './parse-shared.js';
import {
  parseCommands,
  parseManifestPlugins,
  parseNamedRecord,
  parseTemplates,
  parseWorkflows,
} from './parse-spec.js';
import type {
  CommandDeclaration,
  DependencyRecipeDefinition,
  FeatureDefinition,
  GuardDefinition,
  MergeRuleDefinition,
  PostActionDefinition,
  PresetDefinition,
  ResolverDefinition,
  RuntimeManifest,
  RuntimeSchemaDeclaration,
  WorkflowDefinition,
} from './types.js';
import { DEFAULT_SCHEMA_DISCOVERY } from './types.js';

export async function discoverManifestPaths(
  cwd: string,
  discovery: typeof DEFAULT_SCHEMA_DISCOVERY,
  explicitEntries: string[] = [],
): Promise<{ paths: string[]; warnings: string[] }> {
  const manifestPaths = new Set<string>();
  const warnings: string[] = [];

  for (const entry of explicitEntries) {
    const absolutePath = resolve(cwd, entry);
    if (await fileExists(absolutePath)) {
      manifestPaths.add(absolutePath);
    } else {
      warnings.push(`schema entry not found: ${entry}`);
    }
  }

  for (const fileName of discovery.files) {
    const absolutePath = resolve(cwd, fileName);
    if (await fileExists(absolutePath)) {
      manifestPaths.add(absolutePath);
    }
  }

  for (const dirName of discovery.dirs) {
    const absoluteDir = resolve(cwd, dirName);
    if (!(await fileExists(absoluteDir))) {
      continue;
    }
    const entries = await readdir(absoluteDir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isFile()) {
        continue;
      }
      const extension = extname(entry.name).toLowerCase();
      if (!discovery.allowExtensions.includes(extension)) {
        continue;
      }
      manifestPaths.add(resolve(absoluteDir, entry.name));
    }
  }

  return { paths: [...manifestPaths].sort(), warnings };
}

/**
 * 加载 runtime manifest，同时把兼容字段名和内联声明统一归一化。
 * 这里不做命令树构建，只负责把输入整理成稳定的中间结构。
 */
export async function loadRuntimeManifest(cwd: string, filePath: string): Promise<RuntimeManifest> {
  const loaded = await loadConfigModule(filePath);
  const value = asRecord(loaded);
  const runtimeCommands = Array.isArray(value.runtimeCommands)
    ? value.runtimeCommands
    : Array.isArray(value.runtime_commands)
      ? value.runtime_commands
      : [];

  return {
    commands: parseCommands(cwd, value.commands),
    plugins: parseManifestPlugins(value.plugins),
    templates: parseTemplates(value.templates),
    workflows: parseWorkflows(value.workflows),
    presets: parseNamedRecord<PresetDefinition>(value.presets),
    features: parseNamedRecord<FeatureDefinition>(value.features),
    dependencyRecipes: parseNamedRecord<DependencyRecipeDefinition>(value.dependencyRecipes),
    mergeRules: parseNamedRecord<MergeRuleDefinition>(value.mergeRules),
    guards: parseNamedRecord<GuardDefinition>(value.guards),
    resolvers: parseNamedRecord<ResolverDefinition>(value.resolvers),
    postActions: parseNamedRecord<PostActionDefinition>(value.postActions),
    runtimeCommands: runtimeCommands.map((item) => {
      const command = asRecord(item);
      return {
        mount: typeof command.mount === 'string' ? command.mount : '',
        runtime: asRecord(command.runtime),
        options: asRecord(command.options),
        command: parseCommandPackMeta(command.command),
        commands: parseCommands(cwd, command.commands),
        schemas: Array.isArray(command.schemas)
          ? command.schemas.map((schema) => asRecord(schema) as RuntimeSchemaDeclaration)
          : [],
      };
    }),
  };
}

export async function resolveSchemaCommands(
  cwd: string,
  mount: string,
  schemas: RuntimeSchemaDeclaration[] | undefined,
  declaredPlugins: unknown[],
): Promise<{ commands: CommandDeclaration[]; warnings: string[]; events: BridgeEvent[] }> {
  const warnings: string[] = [];
  const events: BridgeEvent[] = [];
  const commands: CommandDeclaration[] = [];
  if (!Array.isArray(schemas) || schemas.length === 0) {
    return { commands, warnings, events };
  }

  const declarations = mergePluginDeclarations(...declaredPlugins);
  const loadedConfig = await loadLanConfig(cwd);
  const policy = createDefaultPluginSecurityPolicy(loadedConfig.config);

  for (const schema of schemas) {
    const pluginRef = typeof schema.plugin === 'string' ? schema.plugin : null;
    const method = typeof schema.method === 'string' ? schema.method : null;
    if (!pluginRef || !method) {
      warnings.push(
        `skip schema converter on mount \`${mount}\`: each schema entry must include plugin and method`,
      );
      continue;
    }

    const declaration = declarations.find(
      (candidate) => candidate.package === pluginRef || candidate.name === pluginRef,
    );
    if (!declaration) {
      warnings.push(
        `skip schema converter \`${pluginRef}.${method}\` on mount \`${mount}\`: plugin is not declared in config or schema plugins`,
      );
      continue;
    }

    try {
      const plugin = await loadCommandPlugin(cwd, declaration, policy, method);
      const handled = await plugin.handle(method, { cwd, mount, schema }, { cwd });
      if (!handled) {
        warnings.push(
          `skip schema converter \`${pluginRef}.${method}\` on mount \`${mount}\`: plugin returned no result`,
        );
        continue;
      }

      const result = asRecord(handled.result);
      commands.push(...parseCommands(cwd, result.commands));
      warnings.push(...stringArray(result.warnings, []));
      events.push(...handled.events);
    } catch (error) {
      warnings.push(
        `skip schema converter \`${pluginRef}.${method}\` on mount \`${mount}\`: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
  }

  return { commands, warnings, events };
}

export function normalizeSchemaDiscovery(value: unknown): typeof DEFAULT_SCHEMA_DISCOVERY {
  const object = asRecord(value);
  return {
    files: stringArray(object.files, DEFAULT_SCHEMA_DISCOVERY.files),
    dirs: stringArray(object.dirs, DEFAULT_SCHEMA_DISCOVERY.dirs),
    allowExtensions: stringArray(
      object.allowExtensions,
      DEFAULT_SCHEMA_DISCOVERY.allowExtensions,
    ).map((item) => item.toLowerCase()),
  };
}

export function normalizeSchemaEntries(value: unknown): string[] {
  if (typeof value === 'string' && value.trim()) {
    return [value.trim()];
  }
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter((item): item is string => typeof item === 'string')
    .map((item) => item.trim())
    .filter(Boolean);
}

export function mergeWorkflowDefinitions(
  ...parts: Array<Record<string, WorkflowDefinition> | undefined>
): Record<string, WorkflowDefinition> {
  return Object.assign({}, ...parts);
}
