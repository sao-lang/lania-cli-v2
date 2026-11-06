/**
 * Rollup adapter，负责构建与 watch 结果的统一封装。
 *
 * 主要导出：rollupCompilerAdapter。
 */
import path from 'node:path';

import { asRecord, loadToolConfig, normalizeError } from '../../core/runtime.js';
import {
  buildResult,
  compilerAssetEvent,
  compilerDoneEvent,
  compilerIssueEvent,
  compilerStartEvent,
  compilerStatusEvent,
  createCompilerResult,
  fallbackDevResult,
  logEvent,
  resolveCompilerRuntime,
  type CompilerAdapter,
} from '../compiler-shared.js';

function normalizePath(value: string) {
  return value.replaceAll('\\', '/');
}

function outputBytes(output: any): number {
  if (output && typeof output.code === 'string') {
    return Buffer.byteLength(output.code, 'utf8');
  }
  const source = output?.source;
  if (typeof source === 'string') {
    return Buffer.byteLength(source, 'utf8');
  }
  if (source && typeof source === 'object' && typeof source.length === 'number') {
    return source.length;
  }
  return 0;
}

export const rollupCompilerAdapter: CompilerAdapter = {
  tool: 'rollup',
  async handleDev(params, context, emit) {
    const runtime = await resolveCompilerRuntime(params.cwd, 'rollup', context.lanConfig);
    if (!runtime?.watch) {
      return null;
    }

    try {
      const toolConfig = await loadToolConfig(params.cwd, 'rollup');
      const startEvent = compilerStartEvent('rollup', 'dev', 'runtime', true, 'development', {
        host: params.host,
        port: params.port,
        configPath: toolConfig.configPath,
      });
      const startingEvent = compilerStatusEvent(
        'rollup',
        'dev',
        'watching',
        'Starting rollup watch mode',
      );
      const startingLog = logEvent(
        'Starting rollup watch mode',
        context.runtimeWarning ? 'warn' : 'info',
      );
      const warningEvents = context.runtimeWarning
        ? [
            compilerIssueEvent('rollup', 'warning', context.runtimeWarning),
            logEvent(`[rollup] ${context.runtimeWarning}`, 'warn'),
          ]
        : [];
      if (emit) {
        [startEvent, startingEvent, startingLog, ...warningEvents].forEach(emit);
      }

      const watcher = runtime.watch({
        ...toolConfig.config,
        watch: { ...(asRecord(toolConfig.config.watch) ?? {}), skipWrite: false },
      });

      const firstBuildEvents = await new Promise<any[]>((resolve) => {
        const collected: any[] = [];
        const handler = (event: any) => {
          if (event?.code === 'START') {
            const entry = compilerStatusEvent('rollup', 'dev', 'watching', 'Rollup watch started');
            collected.push(entry);
            emit?.(entry);
          }
          if (event?.code === 'BUNDLE_END') {
            const entry = compilerStatusEvent(
              'rollup',
              'dev',
              'watching',
              'Rollup bundle finished',
            );
            collected.push(entry);
            emit?.(entry);
          }
          if (event?.code === 'ERROR') {
            const entry = compilerIssueEvent(
              'rollup',
              'error',
              String(event.error?.message ?? 'rollup watch error'),
            );
            collected.push(entry);
            emit?.(entry);
          }
          if (event?.code === 'END') {
            watcher.off?.('event', handler);
            resolve(collected);
          }
        };
        watcher.on?.('event', handler);
        setTimeout(() => {
          watcher.off?.('event', handler);
          resolve(collected);
        }, 1500).unref?.();
      });

      if (emit) {
        // 首次构建后继续转发后续 rebuild 事件。
        watcher.on?.('event', (event: any) => {
          if (event?.code === 'START') {
            emit(compilerStatusEvent('rollup', 'dev', 'watching', 'Rollup rebuild started'));
          }
          if (event?.code === 'END') {
            emit(compilerStatusEvent('rollup', 'dev', 'watching', 'Rollup rebuild finished'));
          }
          if (event?.code === 'ERROR') {
            emit(
              compilerIssueEvent(
                'rollup',
                'error',
                String(event.error?.message ?? 'rollup watch error'),
              ),
            );
          }
        });
      }

      const doneEvent = compilerDoneEvent('rollup', 'dev', true, 'runtime', {
        longRunning: true,
        host: params.host,
        port: params.port,
      });
      if (emit) {
        emit(doneEvent);
      }

      return {
        result: createCompilerResult('rollup', 'dev', 'runtime', {
          mode: 'development',
          host: params.host,
          port: params.port,
          configPath: toolConfig.configPath,
          longRunning: true,
        }),
        events: emit
          ? []
          : [
              startEvent,
              startingEvent,
              startingLog,
              ...warningEvents,
              ...firstBuildEvents,
              doneEvent,
            ],
        activeCompiler: {
          tool: 'rollup',
          action: 'dev',
          stop: async () => {
            watcher.close?.();
          },
        },
      };
    } catch (error) {
      return fallbackDevResult('rollup', params.host, params.port, normalizeError(error).message);
    }
  },
  async handleBuild(params, context) {
    const runtime = await resolveCompilerRuntime(params.cwd, 'rollup', context.lanConfig);
    if (!runtime?.rollup) {
      return null;
    }

    try {
      const toolConfig = await loadToolConfig(params.cwd, 'rollup');
      let activeCompiler;
      if (params.watch && runtime.watch) {
        const watcher = runtime.watch({
          ...toolConfig.config,
          watch: { ...(asRecord(toolConfig.config.watch) ?? {}), skipWrite: false },
        });
        activeCompiler = {
          tool: 'rollup' as const,
          action: 'build' as const,
          stop: async () => {
            watcher.close();
          },
        };
        return {
          result: createCompilerResult('rollup', 'build', 'runtime', {
            watch: true,
            mode: params.mode,
            outputDir: params.outputDir,
            longRunning: true,
            configPath: toolConfig.configPath,
          }),
          events: [
            compilerStartEvent('rollup', 'build', 'runtime', true, params.mode, {
              outputDir: params.outputDir ?? 'dist',
              configPath: toolConfig.configPath,
            }),
            compilerStatusEvent('rollup', 'build', 'watching', 'Starting rollup watch build'),
            compilerDoneEvent('rollup', 'build', true, 'runtime', {
              watch: true,
              longRunning: true,
              outputDir: params.outputDir ?? 'dist',
            }),
          ],
          activeCompiler,
        };
      }

      const bundle = await runtime.rollup(toolConfig.config);
      const outputConfig = asRecord(toolConfig.config.output);
      const output = outputConfig.dir ?? outputConfig.file ?? params.outputDir ?? 'dist';
      const resolvedOutputDir = params.outputDir ?? (typeof output === 'string' ? output : 'dist');
      const writeResult = await bundle.write({
        ...(asRecord(toolConfig.config.output) ?? {}),
        dir: resolvedOutputDir,
      });

      const outputArray = Array.isArray((writeResult as any)?.output)
        ? (writeResult as any).output
        : [];
      const events: any[] = [
        compilerStartEvent('rollup', 'build', 'runtime', false, params.mode, {
          outputDir: resolvedOutputDir,
          configPath: toolConfig.configPath,
        }),
        compilerStatusEvent('rollup', 'build', 'building', 'Running rollup build'),
      ];
      for (const outputItem of outputArray) {
        const fileName = typeof outputItem.fileName === 'string' ? outputItem.fileName : null;
        if (!fileName) {
          continue;
        }
        const bytes = outputBytes(outputItem);
        const file = normalizePath(
          path.posix.join(normalizePath(resolvedOutputDir), normalizePath(fileName)),
        );
        events.push(compilerAssetEvent('rollup', file, bytes, { outputDir: resolvedOutputDir }));
        events.push({ method: 'event.build_asset', params: { file, bytes } });
      }
      events.push(
        compilerDoneEvent('rollup', 'build', true, 'runtime', {
          watch: false,
          longRunning: false,
          outputDir: resolvedOutputDir,
        }),
      );

      return {
        result: createCompilerResult('rollup', 'build', 'runtime', {
          watch: false,
          mode: params.mode,
          outputDir: resolvedOutputDir,
          longRunning: false,
          configPath: toolConfig.configPath,
        }),
        events,
      };
    } catch (error) {
      return buildResult(
        'rollup',
        params.watch,
        params.mode,
        params.outputDir,
        'fallback',
        normalizeError(error).message,
      );
    }
  },
};
