/**
 * lint 插件入口。
 *
 * 这个模块只处理 bridge 请求编排：
 * - 解析参数
 * - 读取配置
 * - 调度 adaptor 执行
 * - 组装统一结果与事件
 *
 * 各 lint 工具的运行细节拆到 `runners.ts`，公共结果/解析工具拆到 `shared.ts`。
 */
import type { BridgeEvent } from '../../protocol/events.js';
import { loadLanConfig } from '../../core/runtime.js';
import {
  buildResultsByAdaptor,
  buildSummaryByAdaptor,
  formatLintSummary,
  logEvent,
  pickRequestedLinters,
  resolveLintTools,
  runWithConcurrency,
  summarizeResults,
} from './shared.js';
import { runAdaptor } from './runners.js';

export const lintPlugin = {
  name: 'lint',
  methods: ['lint.run'],
  async handle(method: string, params: Record<string, unknown>) {
    if (method !== 'lint.run') {
      return null;
    }

    const cwd = typeof params.cwd === 'string' ? params.cwd : process.cwd();
    const mode = params.mode === 'fix' || params.fix === true ? 'fix' : 'check';
    const fix = mode === 'fix';
    const concurrency = typeof params.concurrency === 'number' ? params.concurrency : 4;
    const groupedOutput = params.groupedOutput === true;
    const lanConfig = await loadLanConfig(cwd);
    const requestedLinters = pickRequestedLinters(params.linters);
    const linters = requestedLinters.length ? requestedLinters : resolveLintTools(lanConfig.config);

    const results = await runWithConcurrency(linters, concurrency, async (adaptor) =>
      runAdaptor(cwd, adaptor, lanConfig.config, fix),
    );

    const summary = summarizeResults(results);
    const summaryByAdaptor = buildSummaryByAdaptor(results);
    const summaryText = formatLintSummary(mode, summary, results);
    const exitCode = summary.errors > 0 ? 1 : 0;

    return {
      result: {
        accepted: true,
        mode,
        fix,
        concurrency,
        groupedOutput,
        summary: {
          ...summary,
          adaptors: results.length,
        },
        summaryByAdaptor,
        summaryText,
        formatter: 'lania.lint.formatter.v1',
        normalizer: 'lania.lint.normalizer.v1',
        resultsByAdaptor: buildResultsByAdaptor(results),
        exitCode,
      },
      events: [
        logEvent('Running lint adaptors'),
        {
          method: 'event.lint_start',
          params: {
            mode,
            cwd,
            fix,
            concurrency,
            adaptors: linters,
          },
        } satisfies BridgeEvent,
        ...results.flatMap((result) =>
          result.files.map(
            (file) =>
              ({
                method: 'event.lint_file',
                params: {
                  adaptor: result.adaptor,
                  filePath: file.filePath,
                  errors: file.errors,
                  warnings: file.warnings,
                  implementation: result.implementation,
                },
              }) satisfies BridgeEvent,
          ),
        ),
        ...results.map(
          (result) =>
            ({
              method: 'event.lint_result',
              params: {
                adaptor: result.adaptor,
                errors: result.errors,
                warnings: result.warnings,
                implementation: result.implementation,
              },
            }) satisfies BridgeEvent,
        ),
        {
          method: 'event.lint_summary',
          params: {
            mode,
            errors: summary.errors,
            warnings: summary.warnings,
            files: summary.files,
            adaptors: results.map((result) => result.adaptor),
            exitCode,
          },
        } satisfies BridgeEvent,
      ],
    };
  },
};
