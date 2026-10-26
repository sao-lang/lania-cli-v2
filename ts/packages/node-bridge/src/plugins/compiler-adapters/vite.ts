/**
 * Vite adapter，负责 dev/build/watch 的 bridge 事件映射。
 *
 * 主要导出：viteCompilerAdapter。
 * 关键点：
 * - 包含文件系统读写/路径解析
 */
import { stat } from 'node:fs/promises';
import path from 'node:path';

import type { BridgeEvent } from '../../protocol/events.js';
import { asRecord, loadToolConfig, normalizeError } from '../../core/runtime.js';
import { createDiagEmitter, type DiagLevel } from '../diag.js';
import {
  buildResult,
  compilerAssetEvent,
  compilerDoneEvent,
  compilerIssueEvent,
  compilerServerReadyEvent,
  compilerStartEvent,
  compilerStatusEvent,
  createCompilerResult,
  fallbackDevResult,
  logEvent,
  resolveCompilerRuntime,
  type CompilerAdapter,
} from '../compiler-shared.js';

function assetSizeBytes(output: any): number | null {
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
  return null;
}

function normalizePath(value: string) {
  return value.replaceAll('\\', '/');
}

export const viteCompilerAdapter: CompilerAdapter = {
  tool: 'vite',
  async handleDev(params, context, emit) {
    const vite = await resolveCompilerRuntime(params.cwd, 'vite', context.lanConfig);
    if (!vite?.createServer) {
      return null;
    }

    const toolConfig = await loadToolConfig(params.cwd, 'vite');
    const buffered: Array<ReturnType<typeof logEvent> | ReturnType<typeof compilerIssueEvent>> = [];
    const record = (event: (typeof buffered)[number]) => {
      buffered.push(event);
      emit?.(event);
    };
    // vite 内嵌在 bridge 进程里，它的日志不会经过子进程劫持，必须从 logger 层转成 `event.diag`，
    // 否则 `lan dev` 里只剩 `event.log`，与 `[app]` 运行期日志无法在视觉上区分（§7.5.3）。
    const diag = createDiagEmitter(emit, { channel: 'dev', tool: 'vite', source: 'tool' });
    const customLogger = vite.createLogger
      ? (() => {
          const logger = vite.createLogger('info', {
            allowClearScreen: false,
          });
          const wrap = (level: DiagLevel) => (msg: string) => {
            const text = String(msg ?? '').trim();
            if (!text) {
              return;
            }
            diag(text, 'stdout', level);
            if (level === 'error') {
              record(compilerIssueEvent('vite', 'error', text));
            } else if (level === 'warn') {
              record(compilerIssueEvent('vite', 'warning', text));
            } else {
              record(logEvent(`[vite] ${text}`, 'info'));
            }
          };
          logger.info = wrap('info');
          logger.warn = wrap('warn');
          logger.error = wrap('error');
          return logger;
        })()
      : undefined;
    const serverConfig = {
      ...(asRecord(toolConfig.config.server) ?? {}),
      host: params.host,
      port: params.port,
      open: params.open,
      ...(params.hmr === null ? {} : { hmr: params.hmr }),
    };
    const config = {
      ...toolConfig.config,
      mode: params.mode ?? undefined,
      logLevel: 'silent',
      clearScreen: false,
      customLogger: customLogger ?? toolConfig.config.customLogger,
      server: serverConfig,
    };
    const startEvent = compilerStartEvent(
      'vite',
      'dev',
      'runtime',
      true,
      params.mode ?? 'development',
      {
        host: params.host,
        port: params.port,
        configPath: toolConfig.configPath,
      },
    );
    const startingEvent = compilerStatusEvent(
      'vite',
      'dev',
      'starting',
      'Starting vite dev server',
    );
    const startingLog = logEvent(
      'Starting vite dev server',
      context.runtimeWarning ? 'warn' : 'info',
    );
    if (emit) {
      emit(startEvent);
      emit(startingEvent);
      emit(startingLog);
    }

    try {
      const server = await vite.createServer(config);
      await server.listen();
      const resolvedUrl = server.resolvedUrls?.local?.[0] ?? `http://${params.host}:${params.port}`;
      const warningEvents = context.runtimeWarning
        ? [
            compilerIssueEvent('vite', 'warning', context.runtimeWarning),
            logEvent(`[vite] ${context.runtimeWarning}`, 'warn'),
          ]
        : [];
      const readyEvents = [
        compilerServerReadyEvent('vite', resolvedUrl, params.host, params.port),
        {
          method: 'event.dev_url',
          params: {
            url: resolvedUrl,
          },
        } satisfies BridgeEvent,
        compilerDoneEvent('vite', 'dev', true, 'runtime', {
          longRunning: true,
          host: params.host,
          port: params.port,
        }),
      ];
      if (emit) {
        warningEvents.forEach(emit);
        readyEvents.forEach(emit);
      }
      return {
        result: createCompilerResult('vite', 'dev', 'runtime', {
          mode: params.mode ?? 'development',
          host: params.host,
          port: params.port,
          configPath: toolConfig.configPath,
          longRunning: true,
        }),
        events: emit
          ? []
          : [startEvent, startingEvent, startingLog, ...buffered, ...warningEvents, ...readyEvents],
        activeCompiler: {
          tool: 'vite',
          action: 'dev',
          stop: async () => {
            await server.close();
          },
        },
      };
    } catch (error) {
      return fallbackDevResult('vite', params.host, params.port, normalizeError(error).message);
    }
  },
  async handleBuild(params, context) {
    const runtime = await resolveCompilerRuntime(params.cwd, 'vite', context.lanConfig);
    if (!runtime?.build) {
      return null;
    }

    try {
      const toolConfig = await loadToolConfig(params.cwd, 'vite');
      const outDir =
        params.outputDir ??
        (typeof asRecord(toolConfig.config.build)?.outDir === 'string'
          ? (asRecord(toolConfig.config.build)?.outDir as string)
          : null) ??
        'dist';

      const result = await runtime.build({
        ...toolConfig.config,
        mode: params.mode ?? undefined,
        logLevel: 'silent',
        clearScreen: false,
        build: {
          ...(asRecord(toolConfig.config.build) ?? {}),
          watch: params.watch ? {} : undefined,
          outDir: params.outputDir ?? undefined,
        },
      });

      const events = [
        compilerStartEvent('vite', 'build', 'runtime', params.watch, params.mode, {
          outputDir: outDir,
          configPath: toolConfig.configPath,
        }),
        compilerStatusEvent('vite', 'build', 'building', 'Running vite build'),
      ];

      const outputs = Array.isArray(result) ? result : result ? [result] : [];
      const normalizedOutDir = typeof outDir === 'string' ? outDir : 'dist';
      for (const output of outputs) {
        const chunks = Array.isArray((output as any)?.output) ? (output as any).output : [];
        for (const chunk of chunks) {
          const fileName = typeof chunk.fileName === 'string' ? chunk.fileName : null;
          if (!fileName) {
            continue;
          }
          let bytes = assetSizeBytes(chunk);
          if (bytes === null) {
            try {
              const stats = await stat(path.join(params.cwd, normalizedOutDir, fileName));
              bytes = stats.size;
            } catch {
              bytes = 0;
            }
          }
          const relFile = normalizePath(
            path.posix.join(normalizePath(normalizedOutDir), normalizePath(fileName)),
          );
          events.push(compilerAssetEvent('vite', relFile, bytes, { outputDir: normalizedOutDir }));
          events.push({
            method: 'event.build_asset',
            params: {
              file: relFile,
              bytes,
            },
          });
        }
      }

      events.push(
        compilerDoneEvent('vite', 'build', true, 'runtime', {
          watch: params.watch,
          longRunning: params.watch,
          outputDir: normalizedOutDir,
        }),
      );

      return {
        result: createCompilerResult('vite', 'build', 'runtime', {
          watch: params.watch,
          mode: params.mode,
          outputDir: normalizedOutDir,
          longRunning: params.watch,
          configPath: toolConfig.configPath,
        }),
        events,
      };
    } catch (error) {
      return buildResult(
        'vite',
        params.watch,
        params.mode,
        params.outputDir,
        'fallback',
        normalizeError(error).message,
      );
    }
  },
};
