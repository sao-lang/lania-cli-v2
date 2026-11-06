/**
 * `event.diag` 发射器：把被劫持的终端输出变成可分类、可保序的结构化事件。
 *
 * 关键设计与理由（`docs/design/multi-runtime-bridge.zh-CN.md` §7.2）：
 *
 * - **`seq` 是跨流顺序的唯一保证**：stdout / stderr 是两个独立异步流，
 *   各自 `readline` 各自到达。不共享计数器就会乱序 —— 用户看到
 *   「错误」出现在「编译开始」之前。
 * - **`channel` 区分用途**：`build` / `dev` / `lint` 是工具链，`app` 是应用自身日志
 *   （服务端 HTTP 访问日志）。没有它，`VITE ready` 与 `GET /api/user 200` 混在一起。
 * - **禁止把 stderr 一律当 warning**：stderr 里可能是 error、也可能是进度条。
 *   这里按内容启发式推断，推断不出来就降为 `info`（不再冒充 warning）。
 */
import type { BridgeEvent } from '../protocol/events.js';

export type DiagChannel = 'build' | 'dev' | 'lint' | 'app';
export type DiagSource = 'tool' | 'runtime' | 'host';
export type DiagLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface DiagLine {
  source: DiagSource;
  stream: 'stdout' | 'stderr' | 'syslog';
  tool?: string;
  channel: DiagChannel;
  level: DiagLevel;
  text: string;
  seq: number;
  ts: number;
}

/**
 * 单个 bridge 进程内共享的单调计数器。
 *
 * 用模块级单例而不是每个 spawn 各一份：同一个 bridge 进程里所有子进程的
 * stdout/stderr 都往同一个终端写，只有全局单调才能正确合并。
 */
let sequence = 0;

export function nextDiagSeq(): number {
  sequence += 1;
  return sequence;
}

/** 测试用：重置计数器（避免用例之间相互影响）。 */
export function resetDiagSequence(): void {
  sequence = 0;
}

const ERROR_PATTERN = /\b(error|fatal|panic|exception|failed|failure)\b/i;
const WARN_PATTERN = /\b(warn|warning|deprecated)\b/i;

/**
 * 从一行文本推断 level。
 *
 * 刻意保守：推断不出来时给 `info`，宁可少标一个 warning，
 * 也不要像旧实现那样把所有 stderr 都标成 warning。
 */
export function inferLevel(text: string, stream: 'stdout' | 'stderr' | 'syslog'): DiagLevel {
  const trimmed = text.trim();
  if (trimmed.startsWith('error') || trimmed.startsWith('ERROR')) {
    return 'error';
  }
  if (trimmed.startsWith('warning') || trimmed.startsWith('WARNING')) {
    return 'warn';
  }
  if (ERROR_PATTERN.test(trimmed)) {
    return 'error';
  }
  if (WARN_PATTERN.test(trimmed)) {
    return 'warn';
  }
  // stderr 本身不是错误信号（进度条、编译器彩色输出都可能走 stderr），保持 info。
  return stream === 'syslog' ? 'debug' : 'info';
}

export interface DiagEmitterOptions {
  channel: DiagChannel;
  tool?: string;
  source?: DiagSource;
}

/**
 * 只构造一条 `event.diag`，不发送。
 *
 * 存在意义：有些来源（worker 自身的 stdout/stderr）需要**同时**保留旧事件
 * （`event.log` / `event.compiler_issue`）和一帧保真的 `event.diag`。
 * 直接复用 `createDiagEmitter` 会把发送时机与构造耦合在一起，无法先拿到事件再决定。
 */
export function buildDiagLine(
  text: string,
  stream: 'stdout' | 'stderr' | 'syslog',
  options: DiagEmitterOptions,
  level?: DiagLevel,
): DiagLine | null {
  const trimmed = text.trim();
  if (!trimmed) {
    return null;
  }
  return {
    source: options.source ?? 'tool',
    stream,
    channel: options.channel,
    level: level ?? inferLevel(trimmed, stream),
    text: trimmed,
    seq: nextDiagSeq(),
    ts: Date.now(),
    ...(options.tool ? { tool: options.tool } : {}),
  };
}

/**
 * 创建一行一条的发射器。
 *
 * `emit` 可能为 undefined（非流式请求）；此时返回 no-op，调用方无需分支。
 */
export function createDiagEmitter(
  emit: ((event: BridgeEvent) => void) | undefined,
  options: DiagEmitterOptions,
) {
  return (text: string, stream: 'stdout' | 'stderr' | 'syslog' = 'stdout', level?: DiagLevel) => {
    const line = buildDiagLine(text, stream, options, level);
    if (!line) {
      return;
    }
    emit?.({
      method: 'event.diag',
      params: line,
    });
  };
}

/**
 * 按行切分流。
 *
 * `flush()` 必须被调用（进程 close 时），否则最后一行没有换行符的输出会丢。
 */
export function createLineSplitter(emitLine: (line: string) => void) {
  let buffer = '';
  return {
    push(chunk: Buffer | string) {
      buffer += String(chunk);
      const parts = buffer.split(/\r?\n/);
      buffer = parts.pop() ?? '';
      for (const part of parts) {
        emitLine(part);
      }
    },
    flush() {
      if (buffer) {
        emitLine(buffer);
        buffer = '';
      }
    },
  };
}
