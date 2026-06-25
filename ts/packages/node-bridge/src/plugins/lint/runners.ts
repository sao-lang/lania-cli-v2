import { asRecord, loadToolConfig, resolveModuleFromCwd } from '../../core/runtime.js';
import {
  aggregateDiagnostics,
  failedRuntimeResult,
  fallbackMissingAdaptor,
  fallbackStatusResult,
  okStatusResult,
  parseOxlintDiagnostics,
  resolveAdaptorBinaryPath,
  resolvePackageBinaryFromCwd,
  runBinary,
} from './shared.js';
import { createLintRunResult, type LintAdaptor, type LintRunResult } from './types.js';

// 各 adaptor 的调用细节集中在这里，`plugin.ts` 只负责请求编排与事件输出。
export async function runAdaptor(
  cwd: string,
  adaptor: LintAdaptor,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  switch (adaptor) {
    case 'eslint':
      return runEslint(cwd, lanConfig, fix);
    case 'oxlint':
      return runOxlint(cwd, lanConfig, fix);
    case 'prettier':
      return runPrettier(cwd, lanConfig, fix);
    case 'oxfmt':
      return runOxfmt(cwd, lanConfig, fix);
    case 'stylelint':
      return runStylelint(cwd, lanConfig, fix);
    case 'textlint':
      return runTextlint(cwd, lanConfig, fix);
    default:
      return createLintRunResult(adaptor, 'fallback', []);
  }
}

async function runEslint(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.eslint);
  const runtime =
    adaptor.eslint ??
    (await resolveModuleFromCwd<any>(cwd, 'eslint').then((module) => module?.ESLint ?? module));
  if (!runtime) {
    // 保持旧行为：eslint 缺失时在 check 模式下返回 2 条 warning，避免静默跳过。
    return createLintRunResult('eslint', 'fallback', [
      {
        filePath: '.',
        errors: 0,
        warnings: fix ? 0 : 2,
      },
    ]);
  }

  const toolConfig = await loadToolConfig(cwd, 'eslint');
  const eslint = new runtime({
    cwd,
    fix,
    overrideConfig: toolConfig.exists ? toolConfig.config : undefined,
  });
  const results = await eslint.lintFiles(['.']);
  if (fix && runtime.outputFixes) {
    await runtime.outputFixes(results);
  }

  return createLintRunResult(
    'eslint',
    'runtime',
    results.map((result: any) => ({
      filePath: result.filePath ?? '.',
      errors: result.errorCount ?? 0,
      warnings: result.warningCount ?? 0,
    })),
    results.reduce((count: number, result: any) => count + (result.errorCount ?? 0), 0),
    results.reduce((count: number, result: any) => count + (result.warningCount ?? 0), 0),
  );
}

async function runOxlint(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.oxlint);
  const runtimePath =
    resolveAdaptorBinaryPath(cwd, adaptor) ?? resolvePackageBinaryFromCwd(cwd, 'oxlint', 'oxlint');
  if (!runtimePath) {
    return fallbackMissingAdaptor('oxlint');
  }

  const toolConfig = await loadToolConfig(cwd, 'oxlint');
  const args = ['--format', 'json', '--no-error-on-unmatched-pattern'];
  if (toolConfig.configPath) {
    args.push('--config', toolConfig.configPath);
  }
  if (fix) {
    args.push('--fix');
  }
  args.push('.');

  const command = await runBinary(cwd, runtimePath, args);
  const diagnostics = parseOxlintDiagnostics(command.stdout);
  if (command.exitCode > 1 && diagnostics.length === 0) {
    return failedRuntimeResult('oxlint', command.stderr || command.stdout);
  }

  const files = aggregateDiagnostics(diagnostics);
  return createLintRunResult('oxlint', 'runtime', files);
}

async function runPrettier(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.prettier);
  const runtime = adaptor.prettier ?? (await resolveModuleFromCwd<any>(cwd, 'prettier'));
  if (!runtime) {
    return okStatusResult('prettier', 'fallback');
  }

  const version = typeof runtime.version === 'string' ? runtime.version : 'unknown';
  return okStatusResult('prettier', version === 'unknown' ? 'fallback' : 'runtime');
}

async function runOxfmt(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.oxfmt);
  const runtimePath =
    resolveAdaptorBinaryPath(cwd, adaptor) ?? resolvePackageBinaryFromCwd(cwd, 'oxfmt', 'oxfmt');
  if (!runtimePath) {
    return fallbackMissingAdaptor('oxfmt');
  }

  const toolConfig = await loadToolConfig(cwd, 'oxfmt');
  const args = ['--no-error-on-unmatched-pattern'];
  if (!fix) {
    args.push('--list-different');
  }
  if (toolConfig.configPath) {
    args.push('--config', toolConfig.configPath);
  }
  args.push('.');

  const command = await runBinary(cwd, runtimePath, args);
  if (command.exitCode > 1) {
    return failedRuntimeResult('oxfmt', command.stderr || command.stdout);
  }

  const changedFiles = fix
    ? []
    : command.stdout
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean);
  return createLintRunResult(
    'oxfmt',
    'runtime',
    changedFiles.map((filePath) => ({
      filePath,
      errors: 1,
      warnings: 0,
    })),
    fix ? 0 : changedFiles.length,
    0,
  );
}

async function runStylelint(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.stylelint);
  const runtime = adaptor.stylelint ?? (await resolveModuleFromCwd<any>(cwd, 'stylelint'));
  if (!runtime?.lint) {
    return fallbackStatusResult('stylelint', 1);
  }

  const toolConfig = await loadToolConfig(cwd, 'stylelint');
  const result = await runtime.lint({
    cwd,
    fix,
    files: ['**/*.{css,scss,less}'],
    config: toolConfig.exists ? toolConfig.config : undefined,
  });
  const fileResults = Array.isArray(result.results) ? result.results : [];
  return createLintRunResult(
    'stylelint',
    'runtime',
    fileResults.map((file: any) => ({
      filePath: file.source ?? '.',
      errors: Array.isArray(file.warnings)
        ? file.warnings.filter((warning: any) => warning.severity === 'error').length
        : 0,
      warnings: Array.isArray(file.warnings)
        ? file.warnings.filter((warning: any) => warning.severity !== 'error').length
        : 0,
    })),
    fileResults.reduce(
      (count: number, file: any) => count + (file.errored ? (file.warnings?.length ?? 1) : 0),
      0,
    ),
    fileResults.reduce(
      (count: number, file: any) =>
        count +
        (Array.isArray(file.warnings)
          ? file.warnings.filter((warning: any) => warning.severity !== 'error').length
          : 0),
      0,
    ),
  );
}

async function runTextlint(
  cwd: string,
  lanConfig: Record<string, unknown>,
  fix: boolean,
): Promise<LintRunResult> {
  const lintAdaptors = asRecord(lanConfig.lintAdaptors);
  const adaptor = asRecord(lintAdaptors.textlint);
  const runtime = adaptor.textlint ?? (await resolveModuleFromCwd<any>(cwd, 'textlint'));
  const engine = runtime?.TextLintEngine
    ? new runtime.TextLintEngine({ cwd, fix })
    : (runtime?.createLinter?.({ cwd, fix }) ?? runtime);
  // textlint 不同版本暴露的入口不同，这里保留原有宽松探测策略。
  const lintFiles =
    typeof engine?.lintFiles === 'function'
      ? engine.lintFiles.bind(engine)
      : typeof engine?.executeOnFiles === 'function'
        ? engine.executeOnFiles.bind(engine)
        : null;

  if (!lintFiles) {
    return fallbackStatusResult('textlint', 1);
  }

  const results = await lintFiles(['**/*.{md,txt}']);
  const fileResults = (
    Array.isArray(results) ? results : (asRecord(results).results ?? [])
  ) as any[];
  return createLintRunResult(
    'textlint',
    'runtime',
    fileResults.map((file: any) => ({
      filePath: file.filePath ?? '.',
      errors: Array.isArray(file.messages)
        ? file.messages.filter((message: any) => message.severity === 2).length
        : 0,
      warnings: Array.isArray(file.messages)
        ? file.messages.filter((message: any) => message.severity !== 2).length
        : 0,
    })),
    fileResults.reduce(
      (count: number, file: any) =>
        count +
        (Array.isArray(file.messages)
          ? file.messages.filter((message: any) => message.severity === 2).length
          : 0),
      0,
    ),
    fileResults.reduce(
      (count: number, file: any) =>
        count +
        (Array.isArray(file.messages)
          ? file.messages.filter((message: any) => message.severity !== 2).length
          : 0),
      0,
    ),
  );
}
