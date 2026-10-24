import { spawn, type ChildProcess } from 'node:child_process';

import type { BridgeEvent } from '../../protocol/events.js';
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
    const stdout = createLineEmitter((line) => push(logEvent(line, 'info')));
    const stderr = createLineEmitter((line) => {
      stderrTail = line;
      push(compilerIssueEvent(runtime, 'warning', line));
    });
    child.stdout?.on('data', stdout.push);
    child.stderr?.on('data', stderr.push);
    child.on('error', (error) => {
      resolve({ exitCode: 1, error: error.message, stderrTail });
    });
    child.on('close', (code) => {
      stdout.flush();
      stderr.flush();
      resolve({ exitCode: code ?? 1, error: null, stderrTail });
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
  }
  push(compilerDoneEvent(runtime, 'build', success, 'runtime', { runtime }));
  return {
    result: { ...createCompilerResult(runtime, 'build', 'runtime', {}), success, runtime },
    events: emit ? [] : events,
  };
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
    const stdout = createLineEmitter((line) => {
      push(logEvent(line, 'info'));
      scheduleReady();
    });
    const stderr = createLineEmitter((line) => {
      push(compilerIssueEvent(runtime, 'warning', line));
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
