import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const docsRoot = path.join(repoRoot, 'docs');

const keep = new Set([path.join(repoRoot, 'README.md'), path.join(repoRoot, 'README.zh-CN.md')]);

const ignoreDirs = new Set(['node_modules', 'dist', 'target', '.git']);

function isIgnoredPath(absolutePath) {
  const rel = path.relative(repoRoot, absolutePath);
  if (rel.startsWith('npm/cli/lib/node-bridge')) return true; // generated payload
  return rel.split(path.sep).some((seg) => ignoreDirs.has(seg));
}

function listMarkdownFiles(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (isIgnoredPath(full)) continue;
    if (entry.isDirectory()) {
      out.push(...listMarkdownFiles(full));
      continue;
    }
    if (entry.isFile() && entry.name.endsWith('.md')) {
      out.push(full);
    }
  }
  return out;
}

function ensureDir(dir) {
  fs.mkdirSync(dir, { recursive: true });
}

function uniquePath(targetPath) {
  if (!fs.existsSync(targetPath)) return targetPath;
  const parsed = path.parse(targetPath);
  for (let i = 1; i < 1000; i += 1) {
    const next = path.join(parsed.dir, `${parsed.name}.${i}${parsed.ext}`);
    if (!fs.existsSync(next)) return next;
  }
  throw new Error(`Unable to allocate unique path for ${targetPath}`);
}

function destinationFor(filePath) {
  const rel = path.relative(repoRoot, filePath);
  if (rel.startsWith('docs' + path.sep)) {
    // Already under docs; keep in place.
    return null;
  }
  if (rel.startsWith('.ai' + path.sep)) {
    return path.join(docsRoot, 'ai', rel.slice('.ai/'.length));
  }
  if (rel.startsWith('.changeset' + path.sep)) {
    return path.join(docsRoot, 'changesets', rel.slice('.changeset/'.length));
  }
  // Default: move to docs root with a prefix so we don't pollute the top too much.
  return path.join(docsRoot, rel);
}

function main() {
  ensureDir(docsRoot);

  const markdownFiles = listMarkdownFiles(repoRoot)
    .filter((p) => !keep.has(p))
    .map((p) => path.resolve(p));

  const planned = [];
  for (const filePath of markdownFiles) {
    const dest = destinationFor(filePath);
    if (!dest) continue;
    planned.push([filePath, uniquePath(dest)]);
  }

  if (planned.length === 0) {
    console.log('[md-move] no markdown files to move');
    return;
  }

  for (const [src, dst] of planned) {
    ensureDir(path.dirname(dst));
    fs.renameSync(src, dst);
  }

  console.log(`[md-move] moved ${planned.length} markdown file(s) under docs/`);
}

main();
