/**
 * Webpack adapter，负责 dev/build/watch 的 bridge 事件映射。
 *
 * 主要导出：webpackCompilerAdapter。
 * 关键点：
 * - 包含 JSON 协议/序列化
 */
import path from 'node:path';

import { asRecord, loadToolConfig, normalizeError } from '../../core/runtime.js';
import {
  buildResult,
  compilerAssetEvent,
  compilerDoneEvent,
  compilerIssueEvent,
  compilerServerReadyEvent,
  compilerStartEvent,
  compilerStatusEvent,
  createCompilerResult,
  logEvent,
  type CompilerAdapter,
  resolveCompilerRuntime,
} from '../compiler-shared.js';

function normalizePath(value: string) {
  return value.replaceAll('\\', '/');
}

function resolveOutputDir(cwd: string, config: Record<string, unknown>, override: string | null) {
  if (override) {
    return override;
  }
  const output = asRecord(config.output);
  const outputPath = typeof output.path === 'string' ? output.path : null;
  if (!outputPath) {
    return 'dist';
  }
  const relative = normalizePath(path.relative(cwd, outputPath));
  if (relative && !relative.startsWith('..') && !path.isAbsolute(relative)) {
    return relative;
  }
  return normalizePath(path.basename(outputPath));
}

export const webpackCompilerAdapter: CompilerAdapter = {
  tool: 'webpack',
  async handleDev(params, context) {
    const originalCwd = process.cwd();
    if (originalCwd !== params.cwd) {
      // Some webpack/babel ecosystem plugins resolve modules relative to `process.cwd()`
      // (for example react-refresh). The compiler worker runs from the bridge package dir,
      // so we chdir to the target project to keep module resolution consistent.
      process.chdir(params.cwd);
    }
    const runtime = await resolveCompilerRuntime(
      params.cwd,
      'webpack',
      context.lanConfig,
    );
    if (!runtime?.webpack) {
      if (process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }
      return null;
    }
    if (!runtime.webpackDevServer) {
      const warning =
        context.runtimeWarning ??
        'webpack dev requires `webpack-dev-server` to be installed in the target project';
      if (process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }
      return {
        result: createCompilerResult('webpack', 'dev', 'fallback', {
          mode: params.mode ?? 'development',
          host: params.host,
          port: params.port,
          longRunning: false,
        }),
        events: [
          compilerStartEvent('webpack', 'dev', 'fallback', false, params.mode ?? 'development', {
            host: params.host,
            port: params.port,
          }),
          compilerStatusEvent('webpack', 'dev', 'starting', 'Unable to start webpack dev server'),
          compilerIssueEvent('webpack', 'error', warning),
          logEvent(`[webpack] ${warning}`, 'warn'),
          compilerDoneEvent('webpack', 'dev', false, 'fallback', {
            longRunning: false,
            host: params.host,
            port: params.port,
          }),
        ],
      };
    }

    try {
      const toolConfig = await loadToolConfig(params.cwd, 'webpack');
      const config = {
        ...toolConfig.config,
        mode: params.mode ?? (toolConfig.config as any).mode ?? 'development',
        infrastructureLogging: { level: 'error' },
      };
      // Ensure relative paths in webpack config (entry, loaders, etc.) resolve from the project root
      // instead of the bridge/worker process cwd.
      if (!(config as any).context) {
        (config as any).context = params.cwd;
      }
      const devServer = {
        ...(asRecord((toolConfig.config as any).devServer) ?? {}),
        host: params.host,
        port: params.port,
        open: params.open,
      };
      (config as any).devServer = devServer;

      const compiler = runtime.webpack(config);
      const Server = runtime.webpackDevServer;
      const server = new Server(devServer, compiler);

      if (typeof server.start === 'function') {
        await server.start();
      } else if (typeof server.startCallback === 'function') {
        await new Promise<void>((resolve, reject) => {
          server.startCallback((error: unknown) => (error ? reject(error) : resolve()));
        });
      } else {
        throw new Error('unsupported webpack-dev-server runtime');
      }

      const url = `http://${params.host}:${params.port}`;
      return {
        result: createCompilerResult('webpack', 'dev', 'runtime', {
          mode: params.mode ?? 'development',
          host: params.host,
          port: params.port,
          configPath: toolConfig.configPath,
          longRunning: true,
        }),
        events: [
          compilerStartEvent('webpack', 'dev', 'runtime', true, params.mode ?? 'development', {
            host: params.host,
            port: params.port,
            configPath: toolConfig.configPath,
          }),
          compilerStatusEvent('webpack', 'dev', 'starting', 'Starting webpack dev server'),
          logEvent(
            'Starting webpack dev server',
            context.runtimeWarning ? 'warn' : 'info',
          ),
          ...(context.runtimeWarning
            ? [
                compilerIssueEvent('webpack', 'warning', context.runtimeWarning),
                logEvent(`[webpack] ${context.runtimeWarning}`, 'warn'),
              ]
            : []),
          compilerServerReadyEvent('webpack', url, params.host, params.port),
          {
            method: 'event.dev_url',
            params: {
              url,
            },
          },
          compilerDoneEvent('webpack', 'dev', true, 'runtime', {
            longRunning: true,
            host: params.host,
            port: params.port,
          }),
        ],
        activeCompiler: {
          tool: 'webpack',
          action: 'dev',
          stop: async () => {
            try {
              if (typeof server.stop === 'function') {
                await server.stop();
              } else if (typeof server.stopCallback === 'function') {
                await new Promise<void>((resolve) => server.stopCallback(() => resolve()));
              }
            } finally {
              if (process.cwd() !== originalCwd) {
                process.chdir(originalCwd);
              }
            }
          },
        },
      };
    } catch (error) {
      if (process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }
      return {
        ...buildResult('webpack', false, params.mode, params.outputDir, 'fallback', normalizeError(error).message),
        result: createCompilerResult('webpack', 'dev', 'fallback', {
          mode: params.mode ?? 'development',
          host: params.host,
          port: params.port,
          longRunning: true,
        }),
      };
    }
  },
  async handleBuild(params, context) {
    const originalCwd = process.cwd();
    if (originalCwd !== params.cwd) {
      process.chdir(params.cwd);
    }
    const runtime = await resolveCompilerRuntime(
      params.cwd,
      'webpack',
      context.lanConfig,
    );
    if (!runtime?.webpack) {
      if (process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }
      return null;
    }

    try {
      const toolConfig = await loadToolConfig(params.cwd, 'webpack');
      const config = {
        ...toolConfig.config,
        mode: params.mode ?? undefined,
        watch: params.watch,
        output: {
          ...(asRecord(toolConfig.config.output) ?? {}),
          path: params.outputDir ?? asRecord(toolConfig.config.output).path ?? undefined,
        },
        infrastructureLogging: { level: 'error' },
      };
      // Same as dev: keep relative entry/context paths consistent with the target project.
      if (!(config as any).context) {
        (config as any).context = params.cwd;
      }
      const compiler = runtime.webpack(config);

      const outputDir = resolveOutputDir(params.cwd, config as any, params.outputDir);
      const events: any[] = [
        compilerStartEvent('webpack', 'build', 'runtime', params.watch, params.mode, {
          outputDir,
          configPath: toolConfig.configPath,
        }),
        compilerStatusEvent('webpack', 'build', params.watch ? 'watching' : 'building', 'Running webpack build'),
      ];

      const collectStats = async () => {
        if (params.watch && compiler.watch) {
          return await new Promise<any>((resolve, reject) => {
            compiler.watch({}, (error: unknown, stats: any) => {
              if (error) {
                reject(error);
                return;
              }
              resolve(stats);
            });
          });
        }
        if (compiler.run) {
          return await new Promise<any>((resolve, reject) => {
            compiler.run((error: unknown, stats: any) => {
              if (error) {
                reject(error);
                return;
              }
              resolve(stats);
            });
          });
        }
        return null;
      };

      const stats = await collectStats();
      const json = stats?.toJson
        ? stats.toJson({
            all: false,
            assets: true,
            warnings: true,
            errors: true,
          })
        : null;
      const assets = Array.isArray(json?.assets) ? json.assets : [];
      for (const asset of assets) {
        const name = typeof asset?.name === 'string' ? asset.name : null;
        if (!name) {
          continue;
        }
        const bytes = typeof asset.size === 'number' ? asset.size : 0;
        const file = normalizePath(path.posix.join(normalizePath(outputDir), normalizePath(name)));
        events.push(compilerAssetEvent('webpack', file, bytes, { outputDir }));
        events.push({ method: 'event.build_asset', params: { file, bytes } });
      }
      const errors = Array.isArray(json?.errors) ? json.errors : [];
      for (const item of errors) {
        const message = typeof item?.message === 'string' ? item.message : JSON.stringify(item);
        events.push(compilerIssueEvent('webpack', 'error', message));
      }
      const warnings = Array.isArray(json?.warnings) ? json.warnings : [];
      for (const item of warnings) {
        const message = typeof item?.message === 'string' ? item.message : JSON.stringify(item);
        events.push(compilerIssueEvent('webpack', 'warning', message));
      }
      events.push(
        compilerDoneEvent('webpack', 'build', errors.length === 0, 'runtime', {
          watch: params.watch,
          longRunning: params.watch,
          outputDir,
        }),
      );

      const activeCompiler = params.watch
        ? {
            tool: 'webpack' as const,
            action: 'build' as const,
            stop: async () => {
              try {
                await new Promise<void>((resolve) => compiler.close?.(() => resolve()));
              } finally {
                if (process.cwd() !== originalCwd) {
                  process.chdir(originalCwd);
                }
              }
            },
          }
        : undefined;
      if (!params.watch && process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }

      return {
        result: createCompilerResult('webpack', 'build', 'runtime', {
          watch: params.watch,
          mode: params.mode,
          outputDir,
          longRunning: params.watch,
          configPath: toolConfig.configPath,
        }),
        events,
        activeCompiler,
      };
    } catch (error) {
      if (process.cwd() !== originalCwd) {
        process.chdir(originalCwd);
      }
      return buildResult(
        'webpack',
        params.watch,
        params.mode,
        params.outputDir,
        'fallback',
        normalizeError(error).message,
      );
    }
  },
};
