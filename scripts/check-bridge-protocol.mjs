#!/usr/bin/env node
/**
 * Bridge Hub 协议契约与内聚边界检查（CI 用）。
 *
 *   node scripts/check-bridge-protocol.mjs              全量检查
 *   node scripts/check-bridge-protocol.mjs --self-test 只验证扫描器本身能抓到违规
 *
 * 三项检查：
 *   1. 生成物漂移（委托 protocol/gen.mjs --check，按 LF 归一化）
 *   2. golden 契约：样例帧的 kind / 字段名 / 错误码必须在 schema 内，且无语言字段
 *   3. 内聚边界：手写 adaptor 不得出现传输骨架关键词或语言字段；
 *      生成的 protocol.* 绑定不得引入语言字段
 */
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const HUB = join(ROOT, 'packages/products/bridge-hub');
const PROTOCOL = join(HUB, 'protocol');
const ADAPTORS = join(HUB, 'adaptors');
const SELF_TEST = process.argv.includes('--self-test');

const schema = JSON.parse(readFileSync(join(PROTOCOL, 'schema.json'), 'utf8'));
const SOURCE_EXT = new Set(['.ts', '.mjs', '.js', '.py', '.go', '.dart', '.rs']);

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const SKELETON_RULES = schema.busInvariant.forbiddenSkeletonKeywords.map((keyword) => ({
  pattern: new RegExp(escapeRegExp(keyword), 'i'),
  why: `手写目录出现骨架关键词 ${keyword}（传输代码必须由 gen.mjs 生成）`,
}));
const LANGUAGE_RULES = schema.busInvariant.forbiddenFrameFields.map((field) => ({
  pattern: new RegExp('\\b' + escapeRegExp(field) + '\\b\\s*[:=]'),
  why: `帧/字段引入语言或运行时字段 ${field}（busInvariant 禁止）`,
}));

const failures = [];
const fail = (message) => failures.push(message);

/** 扫描单个源文件，返回命中的规则说明。 */
function scanFile(fullPath) {
  const hits = [];
  const text = readFileSync(fullPath, 'utf8').replace(/\r\n/g, '\n');
  const lines = text.split('\n');
  for (const rule of [...SKELETON_RULES, ...LANGUAGE_RULES]) {
    const line = lines.findIndex((candidate) => rule.pattern.test(candidate));
    if (line >= 0) hits.push({ rule: rule.why, line: line + 1, file: fullPath });
  }
  return hits;
}

function checkGenerated() {
  try {
    execFileSync(process.execPath, [join(PROTOCOL, 'gen.mjs'), '--check'], { stdio: 'pipe' });
    console.log('[1/3] generated artifacts: OK');
  } catch (error) {
    fail(`generated artifacts are stale: ${String(error.stdout ?? error.message).trim()}`);
  }
}

function checkGolden() {
  const golden = JSON.parse(readFileSync(join(PROTOCOL, 'golden/protocol.json'), 'utf8'));
  const samples = JSON.parse(readFileSync(join(PROTOCOL, 'golden/samples.json'), 'utf8'));
  const frames = new Map(golden.frames.map((frame) => [frame.kind, frame]));
  const codes = new Set(golden.errors.map((item) => item.code));
  const forbidden = new Set(schema.busInvariant.forbiddenFrameFields);

  if (golden.frames.length !== schema.frames.length) {
    fail(`golden frame count ${golden.frames.length} != schema ${schema.frames.length}`);
  }
  if (golden.discriminator !== schema.discriminator)
    fail('golden discriminator differs from schema');
  if (!golden.busInvariant || !golden.busInvariant.statement)
    fail('golden is missing busInvariant');

  for (const frame of golden.frames) {
    for (const field of frame.fields) {
      if (forbidden.has(field.name))
        fail(`frame ${frame.kind} declares forbidden field ${field.name}`);
    }
  }

  for (const sample of samples.frames) {
    const spec = frames.get(sample.kind);
    if (!spec) {
      fail(`sample kind ${sample.kind} not in schema`);
      continue;
    }
    const declared = new Set(spec.fields.map((field) => field.name));
    for (const key of Object.keys(sample)) {
      if (key === schema.discriminator) continue;
      if (!declared.has(key)) fail(`sample ${sample.kind} has undeclared field ${key}`);
      if (forbidden.has(key)) fail(`sample ${sample.kind} has forbidden field ${key}`);
    }
  }
  for (const code of samples.errors) {
    if (!codes.has(code)) fail(`sample error ${code} not declared in schema`);
  }
  console.log(
    `[2/3] golden contract: OK (${golden.frames.length} frames / ${codes.size} error codes)`,
  );
}

function checkBoundary() {
  const problems = [];
  // 只扫 adaptors/<lang>/ 下的手写端点文件（endpoint.*）。
  // 同目录下的 lania-*-bridge/ 是暂停前遗留的 1:1 stdio 桥（阶段 8 过渡对象），
  // 含 stdin / requestId / protocolVersion 等旧骨架，不属于新端点 adaptor，不纳入扫描。
  for (const lang of ['rust', 'ts', 'py', 'go', 'dart']) {
    const dir = join(ADAPTORS, lang);
    if (!existsSync(dir)) {
      fail(`missing hand-written adaptor directory for ${lang}`);
      continue;
    }
    let hasEndpoint = false;
    for (const name of readdirSync(dir)) {
      const full = join(dir, name);
      if (!statSync(full).isFile()) continue;
      if (!SOURCE_EXT.has(full.slice(full.lastIndexOf('.')))) continue;
      if (name.startsWith('endpoint.')) hasEndpoint = true;
      problems.push(...scanFile(full));
    }
    if (!hasEndpoint) fail(`missing hand-written endpoint adaptor for ${lang}`);
  }

  // 生成绑定只允许检查语言字段；runtime.* 是合法骨架，含 stdin/json.loads 属预期。
  for (const lang of ['rust', 'ts', 'py', 'go', 'dart']) {
    const dir = join(PROTOCOL, 'gen', lang);
    if (!existsSync(dir)) {
      fail(`missing generated bindings for ${lang}`);
      continue;
    }
    for (const name of readdirSync(dir)) {
      if (name.includes('runtime')) continue;
      const full = join(dir, name);
      if (!statSync(full).isFile()) continue;
      const text = readFileSync(full, 'utf8').replace(/\r\n/g, '\n');
      for (const rule of LANGUAGE_RULES) {
        if (rule.pattern.test(text))
          problems.push({ rule: rule.why, line: 0, file: join(dir, name) });
      }
    }
  }

  for (const problem of problems) {
    fail(`${problem.file}${problem.line ? ':' + problem.line : ''} — ${problem.rule}`);
  }
  if (!problems.length)
    console.log('[3/3] boundary scan: OK (no skeleton / language fields in hand-written adaptors)');
  return problems;
}

function selfTest() {
  const tmp = join(ROOT, 'target/.tmp/bridge-protocol-self-test');
  rmSync(tmp, { recursive: true, force: true });
  mkdirSync(tmp, { recursive: true });
  const bad = join(tmp, 'endpoint.ts');
  writeFileSync(
    bad,
    'const input = process.stdin;\n' +
      'const x = json.loads("{}");\n' +
      'const frame = { language: "python", runtimeKind: "cpython", requestId: "1", protocolVersion: "1" };\n' +
      'dispatch(frame);\n',
    'utf8',
  );
  const hits = scanFile(bad);
  const expected = [
    'stdin',
    'json.loads',
    'language',
    'runtimeKind',
    'requestId',
    'protocolVersion',
    'dispatch(',
  ];
  const missing = expected.filter((keyword) => !hits.some((hit) => hit.rule.includes(keyword)));
  rmSync(tmp, { recursive: true, force: true });
  if (missing.length) {
    console.error('self-test FAILED: detector missed ' + missing.join(', '));
    process.exit(1);
  }
  console.log(`self-test: OK (detector caught ${hits.length} violations: ${expected.join(', ')})`);
}

if (SELF_TEST) {
  selfTest();
} else {
  checkGenerated();
  checkGolden();
  checkBoundary();
  if (failures.length) {
    for (const failure of failures) console.error('  ✗ ' + failure);
    console.error(`bridge protocol check FAILED (${failures.length} issue(s))`);
    process.exit(1);
  }
  console.log('bridge protocol check: OK');
}
