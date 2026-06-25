/**
 * lint 子模块公共工具。
 *
 * 这里收敛三类逻辑：
 * - adaptor 列表、汇总和事件辅助
 * - 二进制解析与子进程执行
 * - oxlint JSON 诊断归一化
 */
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { extname, join, resolve } from 'node:path';

import type { BridgeEvent } from '../../protocol/events.js';
import {
  createLintRunResult,
  isLintAdaptor,
  type LintAdaptor,
  type LintRunFile,
  type LintRunResult,
  type LintMode,
  type LintSummary,
} from './types.js';

export function resolveLintTools(config: Record<string, unknown>): LintAdaptor[] {
  const lintTools = Array.isArray(config.lintTools) ? config.lintTools.filter(isLintAdaptor) : [];
  return lintTools.length ? lintTools : ['eslint'];
}

export function pickRequestedLinters(value: unknown): LintAdaptor[] {
  return Array.isArray(value) ? value.filter(isLintAdaptor) : [];
}

export function summarizeResults(results: LintRunResult[]): LintSummary {
  return results.reduce(
    (state, result) => {
      state.errors += result.errors;
      state.warnings += result.warnings;
      state.files += result.files.length;
      return state;
    },
    { errors: 0, warnings: 0, files: 0 },
  );
}

export function buildSummaryByAdaptor(results: LintRunResult[]) {
  return Object.fromEntries(
    results.map((result) => [
      result.adaptor,
      {
        errors: result.errors,
        warnings: result.warnings,
        files: result.files.length,
        implementation: result.implementation,
      },
    ]),
  );
}

export function buildResultsByAdaptor(results: LintRunResult[]) {
  return Object.fromEntries(results.map((result) => [result.adaptor, result]));
}

export function formatLintSummary(
  mode: LintMode,
  summary: LintSummary,
  results: LintRunResult[],
): string {
  const action = mode === 'fix' ? 'lint fix' : 'lint check';
  const adaptors = results.map((result) => result.adaptor).join(', ') || 'none';
  return `${action}: ${summary.errors} error(s), ${summary.warnings} warning(s), ${summary.files} file(s), adaptors=${adaptors}`;
}

export async function runWithConcurrency<T, R>(
  items: T[],
  concurrency: number,
  worker: (item: T) => Promise<R>,
): Promise<R[]> {
  const results: R[] = [];
  const limit = Math.max(1, concurrency);
  for (let index = 0; index < items.length; index += limit) {
    const batch = items.slice(index, index + limit);
    results.push(...(await Promise.all(batch.map(worker))));
  }
  return results;
}

export function logEvent(message: string): BridgeEvent {
  return {
    method: 'event.log',
    params: {
      level: 'info',
      message,
    },
  };
}

export function fallbackMissingAdaptor(adaptor: LintAdaptor): LintRunResult {
  return createLintRunResult(adaptor, 'fallback', [{ filePath: '.', errors: 0, warnings: 1 }]);
}

export function failedRuntimeResult(adaptor: LintAdaptor, output: string): LintRunResult {
  return createLintRunResult(adaptor, 'runtime', [
    {
      filePath: output.trim() || '.',
      errors: 1,
      warnings: 0,
    },
  ]);
}

export function fallbackStatusResult(
  adaptor: LintAdaptor,
  warnings: number,
  filePath = '.',
): LintRunResult {
  return createLintRunResult(adaptor, 'fallback', [{ filePath, errors: 0, warnings }]);
}

export function okStatusResult(adaptor: LintAdaptor, implementation: 'runtime' | 'fallback') {
  return createLintRunResult(adaptor, implementation, [{ filePath: '.', errors: 0, warnings: 0 }]);
}

export function resolveAdaptorBinaryPath(
  cwd: string,
  adaptor: Record<string, unknown>,
): string | null {
  const candidate =
    typeof adaptor.bin === 'string'
      ? adaptor.bin
      : typeof adaptor.command === 'string'
        ? adaptor.command
        : null;
  if (!candidate) {
    return null;
  }
  return candidate.startsWith('/') ? candidate : resolve(cwd, candidate);
}

export function resolvePackageBinaryFromCwd(
  cwd: string,
  packageName: string,
  binaryName: string,
): string | null {
  try {
    const requireFromCwd = createRequire(join(cwd, '__lania_bridge__.cjs'));
    const packageJsonPath = requireFromCwd.resolve(`${packageName}/package.json`);
    const packageJson = requireFromCwd(packageJsonPath) as {
      bin?: string | Record<string, string>;
    };
    const binEntry =
      typeof packageJson.bin === 'string' ? packageJson.bin : packageJson.bin?.[binaryName];
    return typeof binEntry === 'string' ? resolve(packageJsonPath, '..', binEntry) : null;
  } catch {
    return null;
  }
}

export async function runBinary(
  cwd: string,
  runtimePath: string,
  args: string[],
): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  // 某些 adaptor 的 bin 入口其实是 JS/TS 脚本，需要显式通过当前 Node 进程拉起。
  const extension = extname(runtimePath).toLowerCase();
  const isNodeScript =
    extension === '.js' ||
    extension === '.cjs' ||
    extension === '.mjs' ||
    extension === '.ts' ||
    extension === '.cts' ||
    extension === '.mts';
  const command = isNodeScript ? process.execPath : runtimePath;
  const commandArgs = isNodeScript ? [runtimePath, ...args] : args;

  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, commandArgs, {
      cwd,
      env: {
        ...process.env,
        FORCE_COLOR: '0',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on('data', (chunk) => {
      stderr += String(chunk);
    });
    child.on('error', reject);
    child.on('close', (exitCode) => {
      resolvePromise({
        stdout,
        stderr,
        exitCode: typeof exitCode === 'number' ? exitCode : 1,
      });
    });
  });
}

export function parseOxlintDiagnostics(output: string): Array<{
  filePath: string;
  severity: 'error' | 'warning';
}> {
  if (!output.trim()) {
    return [];
  }
  try {
    return collectOxlintDiagnostics(JSON.parse(output));
  } catch {
    return [];
  }
}

function collectOxlintDiagnostics(
  value: unknown,
  inheritedFilePath?: string,
): Array<{ filePath: string; severity: 'error' | 'warning' }> {
  if (Array.isArray(value)) {
    return value.flatMap((item) => collectOxlintDiagnostics(item, inheritedFilePath));
  }
  if (!value || typeof value !== 'object') {
    return [];
  }

  const record = value as Record<string, unknown>;
  // 兼容不同 JSON 形态里对“文件路径”字段的命名差异。
  const filePath =
    typeof record.filePath === 'string'
      ? record.filePath
      : typeof record.filename === 'string'
        ? record.filename
        : typeof record.path === 'string'
          ? record.path
          : typeof record.source === 'string'
            ? record.source
            : inheritedFilePath;

  if (Array.isArray(record.messages)) {
    return record.messages.flatMap((item) => collectOxlintDiagnostics(item, filePath));
  }
  if (Array.isArray(record.diagnostics)) {
    return record.diagnostics.flatMap((item) => collectOxlintDiagnostics(item, filePath));
  }
  if (Array.isArray(record.results)) {
    return record.results.flatMap((item) => collectOxlintDiagnostics(item, filePath));
  }

  const severity = normalizeSeverity(record.severity ?? record.level ?? record.kind);
  if (severity && filePath) {
    return [{ filePath, severity }];
  }
  return [];
}

function normalizeSeverity(value: unknown): 'error' | 'warning' | null {
  if (value === 'error' || value === 'deny' || value === 2) {
    return 'error';
  }
  if (value === 'warning' || value === 'warn' || value === 1) {
    return 'warning';
  }
  return null;
}

export function aggregateDiagnostics(
  diagnostics: Array<{ filePath: string; severity: 'error' | 'warning' }>,
): LintRunFile[] {
  const byFile = new Map<string, LintRunFile>();
  for (const diagnostic of diagnostics) {
    const entry = byFile.get(diagnostic.filePath) ?? {
      filePath: diagnostic.filePath,
      errors: 0,
      warnings: 0,
    };
    if (diagnostic.severity === 'error') {
      entry.errors += 1;
    } else {
      entry.warnings += 1;
    }
    byFile.set(diagnostic.filePath, entry);
  }
  return [...byFile.values()];
}
