import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

import { normalizeDiagnostics, type Diagnostic } from '../diagnostics.js';
import { toolchainFor } from '../toolchain.js';
import {
  failedRuntimeResult,
  fallbackMissingAdaptor,
  okStatusResult,
  runBinary,
} from './shared.js';
import {
  parseBlackDiagnostics,
  parseClippyDiagnostics,
  parseDartAnalyzeDiagnostics,
  parseFlake8Diagnostics,
  parseGolangciDiagnostics,
  parseGoVetDiagnostics,
  parseMypyDiagnostics,
  parseRuffDiagnostics,
} from './parsers.js';
import {
  createLintRunResult,
  filesFromDiagnostics,
  type LintAdaptor,
  type LintRunResult,
} from './types.js';

interface ToolSpec {
  name: string;
  versionArgs: string[];
  candidates: string[];
}

/** 把 adaptor id 映射到解析函数（唯一映射点，新增工具只改这里）。 */
export const ADAPTOR_PARSERS: Partial<Record<LintAdaptor, (output: string) => Diagnostic[]>> = {
  ruff: parseRuffDiagnostics,
  black: parseBlackDiagnostics,
  mypy: parseMypyDiagnostics,
  flake8: parseFlake8Diagnostics,
  'golangci-lint': parseGolangciDiagnostics,
  'go-vet': parseGoVetDiagnostics,
  clippy: parseClippyDiagnostics,
  'dart-analyze': parseDartAnalyzeDiagnostics,
};

function pythonCandidates(cwd: string, name: string): string[] {
  const executable = process.platform === 'win32' ? `${name}.exe` : name;
  return [
    path.join(cwd, '.venv', 'Scripts', executable),
    path.join(cwd, '.venv', 'bin', name),
    path.join(cwd, 'venv', 'Scripts', executable),
    path.join(cwd, 'venv', 'bin', name),
  ];
}

/**
 * 先确认工具存在：项目虚拟环境优先，其次 PATH。
 *
 * 这个顺序必须保留（§6.4）：项目 `.venv` 里的工具版本才是项目真正要用的，
 * 直接走 PATH 会拿到全局版本。
 */
function resolveTool(cwd: string, spec: ToolSpec): string | null {
  for (const candidate of spec.candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  const probe = spawnSync(spec.name, spec.versionArgs, {
    cwd,
    stdio: 'ignore',
    env: { ...process.env, FORCE_COLOR: '0' },
  });
  return !probe.error && probe.status === 0 ? spec.name : null;
}

/**
 * 统一收口：诊断 → `LintRunResult`。
 *
 * `files` 由 `diagnostics` **派生**，保证两个字段永不失配。
 */
function toResult(adaptor: LintAdaptor, diagnostics: Diagnostic[]): LintRunResult {
  const normalized = normalizeDiagnostics(diagnostics);
  return createLintRunResult(
    adaptor,
    'runtime',
    filesFromDiagnostics(normalized),
    normalized.filter((item) => item.severity === 'error').length,
    normalized.filter((item) => item.severity === 'warning').length,
    undefined,
    normalized,
  );
}

/** 从 `toolchain.ts`（唯一来源）取该 adaptor 的默认参数。 */
function argsFor(adaptor: LintAdaptor, fallback: string[]): string[] {
  for (const runtime of ['python', 'go', 'rust', 'dart'] as const) {
    for (const tool of toolchainFor(runtime)?.lintTools ?? []) {
      if (tool.id === adaptor) {
        return tool.args;
      }
    }
  }
  return fallback;
}

async function runRuff(cwd: string, fix: boolean): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'ruff',
    versionArgs: ['--version'],
    candidates: pythonCandidates(cwd, 'ruff'),
  });
  if (!binary) {
    return fallbackMissingAdaptor('ruff');
  }
  const args = ['check', '--output-format', 'json'];
  if (fix) {
    args.push('--fix');
  }
  args.push('.');
  const command = await runBinary(cwd, binary, args);
  const diagnostics = parseRuffDiagnostics(command.stdout);
  if (command.exitCode !== 0 && diagnostics.length === 0) {
    return failedRuntimeResult('ruff', command.stderr || command.stdout);
  }
  return toResult('ruff', diagnostics);
}

async function runBlack(cwd: string, fix: boolean): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'black',
    versionArgs: ['--version'],
    candidates: pythonCandidates(cwd, 'black'),
  });
  if (!binary) {
    return fallbackMissingAdaptor('black');
  }
  const args = fix ? ['.'] : ['--check', '.'];
  const command = await runBinary(cwd, binary, args);
  if (command.exitCode > 1) {
    return failedRuntimeResult('black', command.stderr || command.stdout);
  }
  return toResult(
    'black',
    fix ? [] : parseBlackDiagnostics(`${command.stdout}\n${command.stderr}`),
  );
}

async function runMypy(cwd: string): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'mypy',
    versionArgs: ['--version'],
    candidates: pythonCandidates(cwd, 'mypy'),
  });
  if (!binary) {
    return fallbackMissingAdaptor('mypy');
  }
  const command = await runBinary(cwd, binary, argsFor('mypy', ['.']));
  const diagnostics = parseMypyDiagnostics(`${command.stdout}\n${command.stderr}`);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('mypy', command.stderr || command.stdout);
  }
  return toResult('mypy', diagnostics);
}

async function runFlake8(cwd: string): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'flake8',
    versionArgs: ['--version'],
    candidates: pythonCandidates(cwd, 'flake8'),
  });
  if (!binary) {
    return fallbackMissingAdaptor('flake8');
  }
  const command = await runBinary(cwd, binary, argsFor('flake8', ['.']));
  const diagnostics = parseFlake8Diagnostics(`${command.stdout}\n${command.stderr}`);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('flake8', command.stderr || command.stdout);
  }
  return toResult('flake8', diagnostics);
}

async function runGolangci(cwd: string, fix: boolean): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'golangci-lint',
    versionArgs: ['--version'],
    candidates: [],
  });
  if (!binary) {
    return fallbackMissingAdaptor('golangci-lint');
  }
  const args = ['run', '--out-format', 'json'];
  if (fix) {
    args.push('--fix');
  }
  args.push('./...');
  const command = await runBinary(cwd, binary, args);
  const diagnostics = parseGolangciDiagnostics(command.stdout || command.stderr);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('golangci-lint', command.stderr || command.stdout);
  }
  return toResult('golangci-lint', diagnostics);
}

async function runGoVet(cwd: string): Promise<LintRunResult> {
  const binary = resolveTool(cwd, { name: 'go', versionArgs: ['version'], candidates: [] });
  if (!binary) {
    return fallbackMissingAdaptor('go-vet');
  }
  const command = await runBinary(cwd, binary, argsFor('go-vet', ['vet', './...']));
  const diagnostics = parseGoVetDiagnostics(`${command.stdout}\n${command.stderr}`);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('go-vet', command.stderr || command.stdout);
  }
  return toResult('go-vet', diagnostics);
}

async function runClippy(cwd: string): Promise<LintRunResult> {
  const binary = resolveTool(cwd, {
    name: 'cargo',
    versionArgs: ['clippy', '--version'],
    candidates: [],
  });
  if (!binary) {
    return fallbackMissingAdaptor('clippy');
  }
  const command = await runBinary(cwd, binary, ['clippy', '--message-format=json']);
  const diagnostics = parseClippyDiagnostics(`${command.stdout}\n${command.stderr}`);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('clippy', command.stderr || command.stdout);
  }
  return toResult('clippy', diagnostics);
}

async function runDartAnalyze(cwd: string): Promise<LintRunResult> {
  const binary = resolveTool(cwd, { name: 'dart', versionArgs: ['--version'], candidates: [] });
  if (!binary) {
    return fallbackMissingAdaptor('dart-analyze');
  }
  const command = await runBinary(cwd, binary, ['analyze', '--format', 'machine']);
  const diagnostics = parseDartAnalyzeDiagnostics(`${command.stdout}\n${command.stderr}`);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('dart-analyze', command.stderr || command.stdout);
  }
  return toResult('dart-analyze', diagnostics);
}

export async function runLanguageAdaptor(
  adaptor: LintAdaptor,
  cwd: string,
  fix: boolean,
): Promise<LintRunResult | null> {
  switch (adaptor) {
    case 'ruff':
      return await runRuff(cwd, fix);
    case 'black':
      return await runBlack(cwd, fix);
    case 'mypy':
      return await runMypy(cwd);
    case 'flake8':
      return await runFlake8(cwd);
    case 'golangci-lint':
      return await runGolangci(cwd, fix);
    case 'go-vet':
      return await runGoVet(cwd);
    case 'clippy':
      return await runClippy(cwd);
    case 'dart-analyze':
      return await runDartAnalyze(cwd);
    default:
      return null;
  }
}

/** `okStatusResult` 在 adaptor 无文件时使用；此处重导出以保持既有调用点稳定。 */
export { okStatusResult };
