import { readdir, readFile, writeFile } from 'node:fs/promises';
import { basename, extname, join, relative, resolve, sep } from 'node:path';

const repoRoot = resolve(new URL('..', import.meta.url).pathname);

const ignoredDirNames = new Set([
  '.git',
  'node_modules',
  'dist',
  'target',
  '.pnpm',
  '.turbo',
  '.cache',
]);

const isSourceFile = (absolutePath) => {
  const rel = relative(repoRoot, absolutePath);
  if (rel.includes(`${join('rust', 'target')}${sep}`)) {
    return false;
  }
  if (rel.includes(`${join('ts', 'packages')}${sep}`)) {
    return true;
  }
  if (rel.includes(`${join('rust', 'crates')}${sep}`)) {
    return true;
  }
  return false;
};

const shouldProcess = (absolutePath) => {
  if (!isSourceFile(absolutePath)) {
    return false;
  }
  const rel = relative(repoRoot, absolutePath);
  if (!rel.includes(`${sep}src${sep}`)) {
    return false;
  }
  const ext = extname(absolutePath);
  if (ext === '.rs') {
    // User request: don't touch test files (unit/integration).
    // - `.../tests/...` are integration tests
    // - `src/tests.rs` is typically a unit-test-only module
    if (rel.includes(`${sep}tests${sep}`) || basename(absolutePath) === 'tests.rs') {
      return false;
    }
    return true;
  }
  if ((ext === '.ts' || ext === '.tsx') && !absolutePath.endsWith('.d.ts')) {
    // User request: don't touch test files.
    // This excludes common conventions like `*.test.ts`, `*.spec.ts`, and `__tests__`.
    const base = basename(absolutePath);
    if (
      rel.includes(`${sep}__tests__${sep}`) ||
      base.endsWith('.test.ts') ||
      base.endsWith('.spec.ts') ||
      base.endsWith('.test.tsx') ||
      base.endsWith('.spec.tsx')
    ) {
      return false;
    }
    return true;
  }
  return false;
};

function firstNonEmptyLine(content) {
  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.length > 0) {
      return trimmed;
    }
  }
  return '';
}

function hasLeadingDocCommentForRust(content) {
  const lines = content.split(/\r?\n/);
  for (let i = 0; i < Math.min(lines.length, 20); i++) {
    const trimmed = lines[i].trim();
    if (!trimmed) {
      continue;
    }
    return trimmed.startsWith('//!') || trimmed.startsWith('/*!');
  }
  return false;
}

function hasLeadingDocCommentForTs(content) {
  const first = firstNonEmptyLine(content);
  return first.startsWith('/**') || first.startsWith('/*!') || first.startsWith('//');
}

function pickKeyPointsRust(content) {
  const points = [];
  if (
    content.includes('tokio::') ||
    content.includes('async fn') ||
    content.includes('CancellationToken')
  ) {
    points.push('包含异步/超时/取消等控制流');
  }
  if (content.includes('serde') || content.includes('serde_json')) {
    points.push('包含序列化/反序列化与 JSON 结构约定');
  }
  if (content.includes('std::process::Command') || content.includes('process::Command')) {
    points.push('包含子进程/环境变量交互');
  }
  if (
    content.includes('Arc') ||
    content.includes('Mutex') ||
    content.includes('broadcast::') ||
    content.includes('mpsc::')
  ) {
    points.push('包含并发共享状态或消息通道');
  }
  if (content.includes('unsafe ')) {
    points.push('包含 unsafe 代码，修改前需确认安全假设');
  }
  return points.slice(0, 3);
}

function pickKeyPointsTs(content) {
  const points = [];
  if (
    content.includes('node:child_process') ||
    content.includes('child_process') ||
    content.includes('fork(')
  ) {
    points.push('包含子进程/IPC 交互');
  }
  if (content.includes('node:worker_threads') || content.includes('worker_threads')) {
    points.push('包含 worker/隔离执行逻辑');
  }
  if (
    content.includes('node:fs') ||
    content.includes('fs/promises') ||
    content.includes('readFile') ||
    content.includes('readdir')
  ) {
    points.push('包含文件系统读写/路径解析');
  }
  if (
    content.includes('readline') ||
    content.includes('process.stdin') ||
    content.includes('process.stdout')
  ) {
    points.push('包含 stdio 协议/流式读写');
  }
  if (content.includes('JSON.parse') || content.includes('JSON.stringify')) {
    points.push('包含 JSON 协议/序列化');
  }
  return points.slice(0, 3);
}

function parseRustExports(content) {
  const patterns = [
    /\bpub\s+(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bpub\s+struct\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bpub\s+enum\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bpub\s+trait\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bpub\s+type\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bpub\s+const\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
  ];
  const found = [];
  for (const pattern of patterns) {
    for (const match of content.matchAll(pattern)) {
      found.push(match[1]);
      if (found.length >= 10) {
        break;
      }
    }
    if (found.length >= 10) {
      break;
    }
  }
  return [...new Set(found)].slice(0, 6);
}

function parseTsExports(content) {
  const patterns = [
    /\bexport\s+(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bexport\s+class\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bexport\s+const\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bexport\s+type\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bexport\s+interface\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
    /\bexport\s+enum\s+([A-Za-z_][A-Za-z0-9_]*)\b/g,
  ];
  const found = [];
  for (const pattern of patterns) {
    for (const match of content.matchAll(pattern)) {
      found.push(match[1]);
      if (found.length >= 10) {
        break;
      }
    }
    if (found.length >= 10) {
      break;
    }
  }
  return [...new Set(found)].slice(0, 6);
}

function buildRustHeader(relPath, exports, points) {
  const lines = [`//! ${relPath}：模块说明。`];
  if (exports.length > 0) {
    lines.push('//!');
    lines.push(`//! 主要导出：${exports.join('、')}。`);
  }
  if (points.length > 0) {
    lines.push('//!');
    lines.push('//! 关键点：');
    for (const p of points) {
      lines.push(`//! - ${p}`);
    }
  }
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function buildTsHeader(relPath, exports, points) {
  const lines = ['/**', ` * ${relPath}：模块说明。`];
  if (exports.length > 0) {
    lines.push(' *');
    lines.push(` * 主要导出：${exports.join('、')}。`);
  }
  if (points.length > 0) {
    lines.push(' *');
    lines.push(' * 关键点：');
    for (const p of points) {
      lines.push(` * - ${p}`);
    }
  }
  lines.push(' */');
  lines.push('');
  return `${lines.join('\n')}\n`;
}

async function walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const results = [];
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (ignoredDirNames.has(entry.name)) {
        continue;
      }
      results.push(...(await walk(join(dir, entry.name))));
      continue;
    }
    results.push(join(dir, entry.name));
  }
  return results;
}

async function main() {
  const files = (await walk(repoRoot)).filter(shouldProcess);
  let modified = 0;

  for (const filePath of files) {
    const content = await readFile(filePath, 'utf8');
    const ext = extname(filePath);
    const relPath = relative(repoRoot, filePath).replaceAll('\\', '/');

    if (ext === '.rs') {
      if (hasLeadingDocCommentForRust(content)) {
        continue;
      }
      const exports = parseRustExports(content);
      const points = pickKeyPointsRust(content);
      const header = buildRustHeader(relPath, exports, points);
      await writeFile(filePath, header + content, 'utf8');
      modified += 1;
      continue;
    }

    if (ext === '.ts' || ext === '.tsx') {
      if (hasLeadingDocCommentForTs(content)) {
        continue;
      }
      const exports = parseTsExports(content);
      const points = pickKeyPointsTs(content);
      const header = buildTsHeader(relPath, exports, points);
      await writeFile(filePath, header + content, 'utf8');
      modified += 1;
      continue;
    }
  }

  process.stdout.write(`updated ${modified} files\n`);
}

await main();
