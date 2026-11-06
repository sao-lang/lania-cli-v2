import { spawn, type ChildProcess } from 'node:child_process';

import type { BridgeEvent } from '../../protocol/events.js';
import { createDiagEmitter, createLineSplitter } from '../diag.js';
import {
  compilerDoneEvent,
  compilerIssueEvent,
  compilerStartEvent,
  compilerStatusEvent,
  createCompilerResult,
  logEvent,
  type CompilerExecution,
  type Tool,
} from '../compiler-shared.js';
import type { Diagnostic } from '../diagnostics.js';
import { nextDiagSeq } from '../diag.js';
import { parseBunBuildDiagnostics } from '../lint/parsers.js';
import type { SpawnCommand } from './commands.js';

const DEV_READY_TIMEOUT_MS = 1000;
const DEV_OUTPUT_GRACE_MS = 250;

export function failureExecution(
  runtime: Tool,
  action: 'dev' | 'build',
  message: string,
  emit?: (event: BridgeEvent) => void,
): CompilerExecution {
  const events: BridgeEvent[] = [
    compilerStartEvent(runtime, action, 'fallback', action === 'dev', null, { runtime }),
    compilerIssueEvent(runtime, 'error', message),
    compilerDoneEvent(runtime, action, false, 'fallback', { runtime }),
  ];
  if (emit) {
    events.forEach((event) => emit(event));
  }
  return {
    result: {
      ...createCompilerResult(runtime, action, 'fallback', {}),
      success: false,
      error: message,
      runtime,
    },
    events: emit ? [] : events,
  };
}

function createLineEmitter(emit: (line: string) => void) {
  let buffer = '';
  const flushLine = (line: string) => {
    const text = line.trim();
    if (text) {
      emit(text);
    }
  };
  return {
    push(chunk: Buffer | string) {
      buffer += String(chunk);
      const parts = buffer.split(/\r?\n/);
      buffer = parts.pop() ?? '';
      parts.forEach(flushLine);
    },
    flush() {
      if (buffer) {
        flushLine(buffer);
        buffer = '';
      }
    },
  };
}

interface SpawnOutcome {
  exitCode: number;
  error: string | null;
  stderrTail: string;
  /** 完整 stderr：编译器诊断需要整段输出，只用尾巴会丢失除最后一条以外的所有错误。 */
  stderr: string;
}

function spawnAndWait(
  cwd: string,
  command: SpawnCommand,
  events: BridgeEvent[],
  runtime: Tool,
  emit?: (event: BridgeEvent) => void,
): Promise<SpawnOutcome> {
  return new Promise((resolve) => {
    let stderrTail = '';
    let stderrAll = '';
    const push = (event: BridgeEvent) => {
      events.push(event);
      emit?.(event);
    };
    const child = spawn(command.command, command.args, {
      cwd,
      env: { ...process.env, ...command.env },
      stdio: ['ignore', 'pipe', 'pipe'],
      // POSIX 下建立独立进程组，stop 时可整组终止（go run/cargo run 会再派生子进程）。
      detached: process.platform !== 'win32',
    });
    // 被劫持的终端输出统一走 `event.diag`（结构化、带 channel 与跨流 seq），
    // 不再压平成 `event.log` / 更不把 stderr 一律当 warning（§7.2）。
    const diag = createDiagEmitter(emit, { channel: 'build', tool: runtime, source: 'tool' });
    const stdout = createLineSplitter((line) => {
      diag(line, 'stdout');
      push(logEvent(line, 'info'));
    });
    const stderr = createLineSplitter((line) => {
      stderrTail = line;
      stderrAll = stderrAll ? `${stderrAll}\n${line}` : line;
      diag(line, 'stderr');
    });
    child.stdout?.on('data', stdout.push);
    child.stderr?.on('data', stderr.push);
    child.on('error', (error) => {
      resolve({ exitCode: 1, error: error.message, stderrTail, stderr: stderrAll });
    });
    child.on('close', (code) => {
      stdout.flush();
      stderr.flush();
      resolve({ exitCode: code ?? 1, error: null, stderrTail, stderr: stderrAll });
    });
  });
}

export async function runSpawnBuild(
  runtime: Tool,
  command: SpawnCommand,
  cwd: string,
  emit?: (event: BridgeEvent) => void,
): Promise<CompilerExecution> {
  const events: BridgeEvent[] = [];
  const push = (event: BridgeEvent) => {
    events.push(event);
    emit?.(event);
  };
  push(compilerStartEvent(runtime, 'build', 'runtime', false, null, { runtime }));
  push(compilerStatusEvent(runtime, 'build', 'building', `Running ${command.command}`));
  const outcome = await spawnAndWait(cwd, command, events, runtime, emit);
  const success = outcome.error === null && outcome.exitCode === 0;
  if (!success) {
    const message = outcome.error ?? (outcome.stderrTail || `exit code ${outcome.exitCode}`);
    push(compilerIssueEvent(runtime, 'error', message));
    // 结构化诊断（§7.3）：编译器错误必须带文件/行/列，只留一行 stderr 尾巴等于丢失全部定位信息。
    for (const event of compilerDiagnosticEvents(command.command, outcome.stderr, 'build')) {
      push(event);
    }
  }
  push(compilerDoneEvent(runtime, 'build', success, 'runtime', { runtime }));
  return {
    result: {
      ...createCompilerResult(runtime, 'build', 'runtime', {}),
      success,
      runtime,
      diagnostics: compilerDiagnostics(command.command, outcome.stderr),
    },
    events: emit ? [] : events,
  };
}

/**
 * 各运行时编译器 stderr → `Diagnostic[]`。
 *
 * 目前只有 `bun build` 有稳定可解析的文本形态；其余运行时先返回空数组，
 * 等各自的解析器落地（B9/B10/B11）再补，避免这里出现「猜格式」的分支。
 *
 * 注意：`runtime` 是 `Tool`（`vite`/`python`/`go`…），不含 `bun` ——
 * bun/deno 走 JS 簇，暂未注册 spawn 适配器，所以这里的 `tool` 由调用方传入
 * 实际执行的命令名，避免用 `runtime` 反推。
 */
function compilerDiagnostics(tool: string, stderr: string): Diagnostic[] {
  return tool === 'bun' ? parseBunBuildDiagnostics(stderr) : [];
}

function compilerDiagnosticEvents(
  tool: string,
  stderr: string,
  channel: 'build' | 'dev',
): BridgeEvent[] {
  return compilerDiagnostics(tool, stderr).map(
    (diagnostic) =>
      ({
        method: 'event.diag',
        params: {
          source: 'tool',
          stream: 'syslog',
          tool: diagnostic.tool,
          channel,
          level: diagnostic.severity === 'error' ? 'error' : 'warn',
          text: diagnostic.message,
          seq: nextDiagSeq(),
          ts: Date.now(),
          fields: {
            file: diagnostic.file,
            line: diagnostic.range?.start.line,
            col: diagnostic.range?.start.col,
            id: diagnostic.id,
          },
        },
      }) satisfies BridgeEvent,
  );
}

export async function startSpawnDev(
  runtime: Tool,
  command: SpawnCommand,
  cwd: string,
  emit?: (event: BridgeEvent) => void,
): Promise<CompilerExecution> {
  const events: BridgeEvent[] = [];
  const push = (event: BridgeEvent) => {
    events.push(event);
    emit?.(event);
  };
  push(compilerStartEvent(runtime, 'dev', 'runtime', true, null, { runtime }));
  push(compilerStatusEvent(runtime, 'dev', 'starting', `Starting ${command.command}`));

  return await new Promise((resolve) => {
    let settled = false;
    let readinessTimer: NodeJS.Timeout | undefined;
    const child = spawn(command.command, command.args, {
      cwd,
      env: { ...process.env, ...command.env },
      stdio: ['ignore', 'pipe', 'pipe'],
      // POSIX 下建立独立进程组，stop 时可整组终止（go run/cargo run 会再派生子进程）。
      detached: process.platform !== 'win32',
    });

    const finishRunning = () => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      clearTimeout(readinessTimer);
      // 从这里开始，子进程的输出就是「应用自身」的日志了。
      serverReady = true;
      push(compilerStatusEvent(runtime, 'dev', 'watching', 'dev command is running'));
      push(compilerDoneEvent(runtime, 'dev', true, 'runtime', { runtime, longRunning: true }));
      resolve({
        result: {
          ...createCompilerResult(runtime, 'dev', 'runtime', {}),
          success: true,
          longRunning: true,
          runtime,
        },
        events: emit ? [] : events,
        activeCompiler: { tool: runtime, action: 'dev', stop: () => killProcessTree(child) },
      });
    };

    // 首个输出后留一个很短的 grace：编译错误通常会在输出后立即退出，
    // 不能一看到 stderr 就宣布 dev server 已就绪。
    const scheduleReady = () => {
      if (!settled && !readinessTimer) {
        readinessTimer = setTimeout(finishRunning, DEV_OUTPUT_GRACE_MS);
      }
    };
    // dev 场景：工具链启动日志属于 `dev`，服务端在启动完成后继续打印的
    // HTTP 访问日志属于**应用自身** —— 两者都用 `event.diag` 但 channel 不同，
    // 上层才能用 `[dev]` / `[app]` 前缀区分（§7.5.3）。
    let serverReady = false;
    const diag = createDiagEmitter(emit, { channel: 'dev', tool: runtime, source: 'tool' });
    const pushDiag = (line: string, stream: 'stdout' | 'stderr') => {
      if (serverReady) {
        // 服务启动后这里就是应用自己的输出。
        createDiagEmitter(emit, { channel: 'app', tool: runtime, source: 'runtime' })(line, stream);
        return;
      }
      diag(line, stream);
    };
    const stdout = createLineEmitter((line) => {
      push(logEvent(line, 'info'));
      pushDiag(line, 'stdout');
      scheduleReady();
    });
    const stderr = createLineEmitter((line) => {
      pushDiag(line, 'stderr');
      scheduleReady();
    });
    child.stdout?.on('data', stdout.push);
    child.stderr?.on('data', stderr.push);

    child.on('error', () => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      clearTimeout(readinessTimer);
      resolve(failureExecution(runtime, 'dev', `command not found: ${command.command}`, emit));
    });
    child.on('close', (code) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      clearTimeout(readinessTimer);
      if (code === 0) {
        push(compilerDoneEvent(runtime, 'dev', true, 'runtime', { runtime, longRunning: false }));
        resolve({
          result: {
            ...createCompilerResult(runtime, 'dev', 'runtime', {}),
            success: true,
            longRunning: false,
            runtime,
          },
          events: emit ? [] : events,
        });
        return;
      }
      resolve(
        failureExecution(
          runtime,
          'dev',
          `dev command exited before ready (code ${code ?? 1})`,
          emit,
        ),
      );
    });

    // 闭包在此之后才可能执行，因此这里用 const 声明即可避免 prefer-const 告警。
    const timer = setTimeout(finishRunning, DEV_READY_TIMEOUT_MS);
  });
}

export async function killProcessTree(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.killed) {
    return;
  }
  const pid = child.pid;
  if (!pid) {
    return;
  }
  if (process.platform === 'win32') {
    await new Promise<void>((resolve) => {
      const killer = spawn('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
      killer.once('close', () => resolve());
      killer.once('error', () => resolve());
    });
    return;
  }
  // detached spawn 让子进程成为独立进程组组长，负 pid 可终止整组。
  try {
    process.kill(-pid, 'SIGTERM');
  } catch {
    child.kill('SIGTERM');
  }
  await new Promise<void>((resolve) => setTimeout(resolve, 2000));
  if (child.exitCode === null) {
    try {
      process.kill(-pid, 'SIGKILL');
    } catch {
      child.kill('SIGKILL');
    }
  }
}
