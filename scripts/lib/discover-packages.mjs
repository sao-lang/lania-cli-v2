/**
 * packages/* 单一事实源的发现与校验（文档 §6.1 / §8 阶段 1）。
 *
 * 无第三方依赖：不引入 ajv，按 schema 的关键约束手工校验，
 * 保证 `pnpm packages:check` 在离线环境也能跑。
 */
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PACKAGES = join(ROOT, 'packages');
const SCHEMA = join(PACKAGES, 'schemas', 'lania.package.schema.json');
const MANIFEST = 'lania.package.json';

const KINDS = new Set(['foundation', 'product', 'command', 'distribution', 'command-runtime']);
const FAMILIES = new Set(['lan', 'runex', 'agent', 'bridge-hub']);
const SHIM_HOSTS = new Set(['lan', 'runex-binary', 'command-binary']);

/** 递归枚举 packages/ 下全部 lania.package.json。 */
export function discoverPackages(root = PACKAGES) {
  const found = [];
  const walk = (dir) => {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir).sort()) {
      if (entry === 'node_modules' || entry === 'target' || entry === 'dist') continue;
      const full = join(dir, entry);
      if (!statSync(full).isDirectory()) continue;
      const manifestPath = join(full, MANIFEST);
      if (existsSync(manifestPath)) {
        found.push({
          dir: full,
          relativeDir: relative(ROOT, full).split(sep).join('/'),
          path: manifestPath,
          manifest: JSON.parse(readFileSync(manifestPath, 'utf8')),
        });
      }
      walk(full);
    }
  };
  walk(root);
  return found;
}

/** 校验单包 manifest，返回问题列表。 */
export function validatePackage(entry) {
  const { manifest, dir, relativeDir } = entry;
  const problems = [];
  const fail = (message) => problems.push(`${relativeDir}: ${message}`);

  for (const field of ['name', 'kind', 'versionFamily', 'version', 'api']) {
    if (manifest[field] === undefined) fail(`缺少必需字段 ${field}`);
  }
  if (manifest.name !== undefined && !/^[a-z][a-z0-9-]*$/.test(manifest.name)) {
    fail(`name 非法：${manifest.name}`);
  }
  if (manifest.kind !== undefined && !KINDS.has(manifest.kind)) fail(`kind 非法：${manifest.kind}`);
  if (manifest.versionFamily !== undefined && !FAMILIES.has(manifest.versionFamily)) {
    fail(`versionFamily 非法：${manifest.versionFamily}`);
  }
  if (manifest.version !== undefined && !/^\d+\.\d+\.\d+$/.test(manifest.version)) {
    fail(`version 非法：${manifest.version}`);
  }
  if (manifest.shimHost !== undefined && !SHIM_HOSTS.has(manifest.shimHost)) {
    fail(`shimHost 非法：${manifest.shimHost}`);
  }

  const rust = manifest.api?.rust;
  if (!manifest.api) {
    fail('缺少 api');
  } else {
    if (!Number.isInteger(manifest.api.protocol) || manifest.api.protocol < 1) {
      fail('api.protocol 必须是 >= 1 的整数');
    }
    // Rust 段是可选的：纯 JS 分发包（如 core 调度器）没有 crate。
    // 一旦声明了 Rust 段，就必须给出 lib 与 api 三元组（文档 §6.1）。
    if (rust !== undefined && !rust?.lib) fail('缺少 api.rust.lib');
    if (rust?.api) {
      for (const key of ['factory', 'spec', 'surface']) {
        if (rust.api[key] === undefined) fail(`缺少 api.rust.api.${key}`);
      }
      if (rust.api.surface !== undefined && !Array.isArray(rust.api.surface)) {
        fail('api.rust.api.surface 必须是数组');
      }
    }
  }

  if (manifest.kind === 'command') {
    for (const field of ['command', 'order', 'describe']) {
      if (manifest[field] === undefined) fail(`command 类必须有 ${field}`);
    }
  }
  if (manifest.command !== undefined && manifest.kind !== 'command') {
    fail('非 command 类不得声明 command');
  }

  // crate 路径存在性
  for (const crate of manifest.rust?.crates ?? []) {
    if (!existsSync(join(dir, crate, 'Cargo.toml'))) fail(`声明的 crate 不存在：${crate}`);
  }
  // npm 包与 bin 存在性
  if (manifest.npm?.name) {
    for (const bin of Object.values(manifest.npm.bin ?? {})) {
      // bin 位于 npm/<pkg>/ 下，manifest 里的路径相对包根
      if (!existsSync(join(dir, bin))) fail(`声明的 npm bin 不存在：${bin}`);
    }
  }
  // describe 与 spec().name 一致性（文档 §6.1 / §10.3 CI 断言）
  if (manifest.describe) {
    const describePath = join(dir, manifest.describe);
    if (!existsSync(describePath)) {
      fail(`describe 产物不存在：${manifest.describe}`);
    } else if (manifest.kind === 'command-runtime') {
      // runtime 是设施而非命令：describe 只要求存在，不做 spec().name 一致性比对。
    } else if (manifest.command) {
      const spec = JSON.parse(readFileSync(describePath, 'utf8'));
      if (spec.name !== manifest.command) {
        fail(`manifest.command=${manifest.command} 与 describe 的 spec().name=${spec.name} 不一致`);
      }
    }
  }
  if (manifest.payload?.nodeBridge === false && (manifest.payload.slices ?? []).length > 0) {
    fail('nodeBridge=false 的包不得声明 payload.slices');
  }
  return problems;
}

/** 跨包一致性：name / command / order 唯一，schema 存在。 */
export function validateAll(entries) {
  const problems = [];
  if (!existsSync(SCHEMA)) problems.push('缺少 packages/schemas/lania.package.schema.json');
  const names = new Set();
  const commands = new Set();
  const orders = new Set();
  for (const entry of entries) {
    problems.push(...validatePackage(entry));
    const { name, command, order } = entry.manifest;
    if (names.has(name)) problems.push(`重复的包名：${name}`);
    names.add(name);
    if (command !== undefined) {
      if (commands.has(command)) problems.push(`重复的命令名：${command}`);
      commands.add(command);
    }
    if (order !== undefined) {
      if (orders.has(order)) problems.push(`重复的 order：${order}`);
      orders.add(order);
    }
  }
  return problems;
}

export { ROOT, PACKAGES, SCHEMA };
