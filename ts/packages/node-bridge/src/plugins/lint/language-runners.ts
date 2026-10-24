import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

import { asRecord } from '../../core/runtime.js';
import {
  aggregateDiagnostics,
  failedRuntimeResult,
  fallbackMissingAdaptor,
  runBinary,
} from './shared.js';
import { createLintRunResult, type LintAdaptor, type LintRunResult } from './types.js';

type Diagnostic = { filePath: string; severity: 'error' | 'warning' };

interface ToolSpec {
  name: string;
  versionArgs: string[];
  candidates: string[];
}

function pythonCandidates(cwd: string, name: string): string[] {
  const executable = process.platform === 'win32' ? `${name}.exe` : name;
  return [
    path.join(cwd, '.venv', 'Scripts', executable),
    path.join(cwd, '.venv', 'bin', name),
    path.join(cwd, 'venv', 'Scripts', executable),
    path.join(cwd, 'venv', 'bin', name),
  ];
}

/** 先确认工具存在：项目虚拟环境优先，其次 PATH。 */
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

function toResult(adaptor: LintAdaptor, diagnostics: Diagnostic[]): LintRunResult {
  return createLintRunResult(adaptor, 'runtime', aggregateDiagnostics(diagnostics));
}

export function parseRuffOutput(output: string): Diagnostic[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(output || '[]');
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) {
    return [];
  }
  return parsed.flatMap((item) => {
    const record = asRecord(item);
    const filePath =
      typeof record.filename === 'string'
        ? record.filename
        : typeof record.file === 'string'
          ? record.file
          : null;
    return filePath ? [{ filePath, severity: 'error' as const }] : [];
  });
}

export function parseBlackOutput(output: string): Diagnostic[] {
  return output
    .split(/\r?\n/)
    .map((line) => {
      const marker = 'would reformat ';
      const index = line.indexOf(marker);
      return index >= 0 ? line.slice(index + marker.length).trim() : '';
    })
    .filter(Boolean)
    .map((filePath) => ({ filePath, severity: 'warning' as const }));
}

export function parseMypyOutput(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^(.+?):(\d+): (error|warning|note):/.exec(line);
    if (!match || match[3] === 'note') {
      continue;
    }
    diagnostics.push({
      filePath: match[1],
      severity: match[3] === 'error' ? 'error' : 'warning',
    });
  }
  return diagnostics;
}

export function parseFlake8Output(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const match = /^(.+?):(\d+):(\d+): ([A-Z]\d+) /.exec(line);
    if (!match) {
      continue;
    }
    diagnostics.push({
      filePath: match[1],
      severity: match[4].startsWith('W') ? 'warning' : 'error',
    });
  }
  return diagnostics;
}

export function parseGolangciOutput(output: string): Diagnostic[] {
  try {
    const parsed = JSON.parse(output);
    const issues = Array.isArray(parsed)
      ? parsed
      : Array.isArray(asRecord(parsed).Issues)
        ? (asRecord(parsed).Issues as unknown[])
        : [];
    const diagnostics: Diagnostic[] = [];
    for (const issue of issues) {
      const record = asRecord(issue);
      const position = asRecord(record.Pos ?? record.pos);
      const filePath =
        typeof position.Filename === 'string'
          ? position.Filename
          : typeof position.filename === 'string'
            ? position.filename
            : null;
      if (!filePath) {
        continue;
      }
      diagnostics.push({
        filePath,
        severity: record.Severity === 'warning' ? 'warning' : 'error',
      });
    }
    return diagnostics;
  } catch {
    const diagnostics: Diagnostic[] = [];
    for (const line of output.split(/\r?\n/)) {
      const match = /^(.+?):(\d+):(\d+): /.exec(line);
      if (match) {
        diagnostics.push({ filePath: match[1], severity: 'error' });
      }
    }
    return diagnostics;
  }
}

export function parseGoVetOutput(output: string): Diagnostic[] {
  return output
    .split(/\r?\n/)
    .map((line) => /^(.+?\.go):(\d+):(\d+): /.exec(line))
    .filter((match): match is RegExpExecArray => Boolean(match))
    .map((match) => ({ filePath: match[1], severity: 'error' as const }));
}

export function parseClippyOutput(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    if (!line.trim()) {
      continue;
    }
    let record: Record<string, unknown>;
    try {
      record = asRecord(JSON.parse(line));
    } catch {
      continue;
    }
    if (record.reason !== 'compiler-message') {
      continue;
    }
    const message = asRecord(record.message);
    const level = message.level;
    if (level !== 'error' && level !== 'warning') {
      continue;
    }
    const spans = Array.isArray(message.spans) ? message.spans : [];
    const filePath = spans
      .map((span) => asRecord(span).file_name)
      .find((value): value is string => typeof value === 'string');
    if (filePath) {
      diagnostics.push({ filePath, severity: level });
    }
  }
  return diagnostics;
}

export function parseDartAnalyzeOutput(output: string): Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const line of output.split(/\r?\n/)) {
    const fields = line.split('|');
    if (fields.length < 4) {
      continue;
    }
    const severity = fields[0];
    const filePath = fields[3];
    if (!filePath) {
      continue;
    }
    if (severity === 'ERROR') {
      diagnostics.push({ filePath, severity: 'error' });
    } else if (severity === 'WARNING') {
      diagnostics.push({ filePath, severity: 'warning' });
    }
  }
  return diagnostics;
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
  const diagnostics = parseRuffOutput(command.stdout);
  if (command.exitCode > 1 && diagnostics.length === 0) {
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
  return toResult('black', fix ? [] : parseBlackOutput(`${command.stdout}\n${command.stderr}`));
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
  const command = await runBinary(cwd, binary, ['.']);
  const diagnostics = parseMypyOutput(`${command.stdout}\n${command.stderr}`);
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
  const command = await runBinary(cwd, binary, ['.']);
  const diagnostics = parseFlake8Output(`${command.stdout}\n${command.stderr}`);
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
  const diagnostics = parseGolangciOutput(command.stdout || command.stderr);
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
  const command = await runBinary(cwd, binary, ['vet', './...']);
  const diagnostics = parseGoVetOutput(`${command.stdout}\n${command.stderr}`);
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
  const diagnostics = parseClippyOutput(`${command.stdout}\n${command.stderr}`);
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
  const diagnostics = parseDartAnalyzeOutput(`${command.stdout}\n${command.stderr}`);
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
