import { asRecord, loadLanConfig } from '../../core/runtime.js';
import {
  discoverManifestPaths,
  loadRuntimeManifest,
  normalizeSchemaDiscovery,
  normalizeSchemaEntries,
} from '../dynamic-commands/parse-manifest.js';
import type { DeclaredTemplateDefinition } from './types.js';

// 这个文件只负责从产品配置/schema 清单中发现“声明过的模板元信息”。
// 它和 runtime 层分离的原因是：
// - runtime 提供实际可执行的模板能力
// - discovery 提供面向产品配置的描述性信息（标题、描述、标签等）
// service 层再把两者合并成最终返回给 bridge 的结果。
export async function loadDeclaredTemplates(
  cwd: string | null,
): Promise<DeclaredTemplateDefinition[]> {
  if (!cwd) {
    return [];
  }

  const loadedConfig = await loadLanConfig(cwd);
  const schemaConfig = asRecord(loadedConfig.config.schema);
  const productConfig = asRecord(loadedConfig.config.product);
  const discovery = normalizeSchemaDiscovery(
    schemaConfig.discovery ?? loadedConfig.config.schemaDiscovery,
  );
  const configuredEntries = normalizeSchemaEntries(
    schemaConfig.entry ?? productConfig.schemaEntry,
  );
  const discovered = await discoverManifestPaths(cwd, discovery, configuredEntries);
  const templates = new Map<string, DeclaredTemplateDefinition>();

  // manifest 中可能通过多个入口重复声明同一个模板 id。
  // 这里用 Map 去重，保证上层看到的是“按模板 id 合并后的最终视图”。
  for (const manifestPath of discovered.paths) {
    const manifest = await loadRuntimeManifest(cwd, manifestPath);
    for (const template of manifest.templates) {
      templates.set(template.id, template);
    }
  }

  return [...templates.values()];
}
