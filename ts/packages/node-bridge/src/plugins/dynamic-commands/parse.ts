import type { BridgePluginResult } from '../../core/bridge-plugin.js';
import type { BridgeEvent } from '../../protocol/events.js';
import { asRecord, loadLanConfig } from '../../core/runtime.js';
import { dirname } from 'node:path';
import {
  discoverManifestPaths,
  loadRuntimeManifest,
  mergeWorkflowDefinitions,
  normalizeSchemaDiscovery,
  normalizeSchemaEntries,
  resolveSchemaCommands,
} from './parse-manifest.js';
import { applyCommandPackMeta, createCommandSpec, sanitizeSegment } from './parse-shared.js';
import { buildManifestCommands } from './parse-spec.js';
import { resolveDynamicRuntimeRoots } from './runtime-context.js';
import { clearRegistriesForCwd } from './state.js';
import type {
  DynamicHandlerRegistration,
  GeneratedCommandSpec,
  ResolveDynamicResult,
  WorkflowDefinition,
} from './types.js';
import { RESERVED_TOP_LEVEL_COMMANDS } from './types.js';

const AUTHOR_COMMANDS_INTERNAL_MOUNT = '__author__';

/**
 * 解析并构建动态命令树。
 * 返回 `commands` 供 Rust 构建 CLI，返回 `handlers` 供执行阶段按 handlerId 回调 Node。
 */
export async function resolveDynamicCommands(
  cwd: string,
  params?: Record<string, unknown>,
): Promise<BridgePluginResult<ResolveDynamicResult>> {
  const runtimeRoots = resolveDynamicRuntimeRoots(cwd, params);
  const configRoot = runtimeRoots.productRoot;
  const loadedConfig = await loadLanConfig(configRoot);
  const schemaConfig = asRecord(loadedConfig.config.schema);
  const productConfig = asRecord(loadedConfig.config.product);
  const extensions = asRecord(loadedConfig.config.extensions);
  if (extensions.dynamicCommands !== true) {
    return {
      result: {
        commands: [],
        handlers: [],
        mounts: [],
        warnings: [],
      },
      events: [],
    };
  }

  const discovery = normalizeSchemaDiscovery(
    schemaConfig.discovery ?? loadedConfig.config.schemaDiscovery,
  );
  const configuredEntries = normalizeSchemaEntries(
    schemaConfig.entry ?? productConfig.schemaEntry,
  );
  const discovered = await discoverManifestPaths(configRoot, discovery, configuredEntries);
  const manifestPaths = discovered.paths;
  const warnings: string[] = [...discovered.warnings];
  const events: BridgeEvent[] = [];
  const handlers: DynamicHandlerRegistration[] = [];
  const mountRoots = new Map<string, GeneratedCommandSpec>();
  const authorRootCommands: GeneratedCommandSpec[] = [];
  const mounts = new Map<string, { rootHandlerId: string; services: Set<string> }>();
  let workflows: Record<string, WorkflowDefinition> = {};
  let sequence = 0;

  // 每次 resolve 都重建 cwd 级 registry，避免跨项目或重复解析时复用旧函数引用。
  clearRegistriesForCwd(cwd);

  for (const manifestPath of manifestPaths) {
    const manifest = await loadRuntimeManifest(cwd, manifestPath);
    const schemaRoot = dirname(manifestPath);
    workflows = mergeWorkflowDefinitions(workflows, manifest.workflows);

    if (manifest.commands.length > 0) {
      const built = buildManifestCommands(
        cwd,
        AUTHOR_COMMANDS_INTERNAL_MOUNT,
        manifest.commands,
        ++sequence,
        {
          schemaRoot,
          workflows,
          plugins: manifest.plugins,
          presets: manifest.presets,
          featuresCatalog: manifest.features,
          dependencyRecipeCatalog: manifest.dependencyRecipes,
          mergeRules: manifest.mergeRules,
          guards: manifest.guards,
          resolvers: manifest.resolvers,
          postActions: manifest.postActions,
        },
      );
      sequence = built.nextSequence;
      authorRootCommands.push(...built.commands);
      handlers.push(...built.handlers);
      warnings.push(...built.warnings);
    }

    for (const runtimeCommand of manifest.runtimeCommands) {
      const mount = sanitizeSegment(runtimeCommand.mount);
      if (!mount) {
        warnings.push(`skip runtime command from ${manifestPath}: mount is required`);
        continue;
      }
      if (RESERVED_TOP_LEVEL_COMMANDS.has(mount)) {
        warnings.push(
          `skip runtime command from ${manifestPath}: mount \`${mount}\` conflicts with a reserved top-level command`,
        );
        continue;
      }

      let root = mountRoots.get(mount);
      if (!root) {
        const rootHandlerId = `dynamic.mount.${mount}`;
        root = createCommandSpec(
          mount,
          runtimeCommand.command?.about ?? `Runtime command pack mounted at ${mount}`,
          rootHandlerId,
        );
        mountRoots.set(mount, root);
        mounts.set(mount, { rootHandlerId, services: new Set() });
      }
      applyCommandPackMeta(root, runtimeCommand.command);

      if (runtimeCommand.schemas) {
        warnings.push(
          `runtimeCommands[].schemas is deprecated; prefer runtimeCommands[].commands, or use a schema-converter plugin during migration (manifest: ${manifestPath})`,
        );
        const converted = await resolveSchemaCommands(
          configRoot,
          mount,
          runtimeCommand.schemas,
          [loadedConfig.config.plugins, manifest.plugins],
        );
        warnings.push(...converted.warnings);
        events.push(...converted.events);
        const convertedBuilt = buildManifestCommands(cwd, mount, converted.commands, ++sequence, {
          schemaRoot,
          workflows,
          plugins: manifest.plugins,
          presets: manifest.presets,
          featuresCatalog: manifest.features,
          dependencyRecipeCatalog: manifest.dependencyRecipes,
          mergeRules: manifest.mergeRules,
          guards: manifest.guards,
          resolvers: manifest.resolvers,
          postActions: manifest.postActions,
        });
        sequence = convertedBuilt.nextSequence;
        for (const command of convertedBuilt.commands) {
          root.subcommands.push(command);
        }
        handlers.push(...convertedBuilt.handlers);
        warnings.push(...convertedBuilt.warnings);
      }

      const declaredCommands = runtimeCommand.commands ?? [];
      const built = buildManifestCommands(cwd, mount, declaredCommands, ++sequence, {
        schemaRoot,
        workflows,
        plugins: manifest.plugins,
        presets: manifest.presets,
        featuresCatalog: manifest.features,
        dependencyRecipeCatalog: manifest.dependencyRecipes,
        mergeRules: manifest.mergeRules,
        guards: manifest.guards,
        resolvers: manifest.resolvers,
        postActions: manifest.postActions,
      });
      sequence = built.nextSequence;
      for (const command of built.commands) {
        root.subcommands.push(command);
      }
      handlers.push(...built.handlers);
      warnings.push(...built.warnings);
    }
  }

  for (const [mount, info] of mounts) {
    const rootHandler = handlers.find((entry) => entry.handlerId === info.rootHandlerId);
    const commands = (mountRoots.get(mount)?.subcommands ?? []).map((command) => command.name).sort();
    if (rootHandler) {
      rootHandler.target = { kind: 'mount_summary', mount, commands };
    } else {
      handlers.push({
        handlerId: info.rootHandlerId,
        method: 'command.invokeDynamic',
        plugin: 'dynamic-commands',
        target: { kind: 'mount_summary', mount, commands },
      });
    }
  }

  if (warnings.length > 0) {
    events.push(
      ...warnings.map(
        (warning) =>
          ({
            method: 'event.log',
            params: {
              level: 'warn',
              message: warning,
            },
          }) satisfies BridgeEvent,
      ),
    );
  }

  return {
    result: {
      commands: [...authorRootCommands, ...mountRoots.values()].sort((left, right) =>
        left.name.localeCompare(right.name),
      ),
      handlers,
      mounts: [...mounts.entries()].map(([mount, info]) => ({
        mount,
        rootHandlerId: info.rootHandlerId,
      })),
      warnings,
    },
    events,
  };
}
export { discoverManifestPaths } from './parse-manifest.js';
