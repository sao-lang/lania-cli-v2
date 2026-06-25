import { spawn } from 'node:child_process';
import { basename } from 'node:path';

import type { DiscoveredCommand, ShellDiscoverySnapshot } from './types.js';

// 与 PATH 扫描不同，这一层专门处理 shell 会话内才能看到的命令来源：
// builtin、alias、function。
// 它只负责“探测与解析”，不负责过滤、排序、限流。
export async function discoverShellCommands(
  shellExecutable: string | null,
  cwd: string,
): Promise<ShellDiscoverySnapshot> {
  if (!shellExecutable) {
    return createEmptyShellSnapshot(null);
  }

  const shellName = detectShellName(shellExecutable);
  if (shellName !== 'zsh' && shellName !== 'bash') {
    return {
      ...createEmptyShellSnapshot(shellExecutable),
      shellName,
      supported: false,
    };
  }

  try {
    const output = await runShellDiscovery(shellExecutable, shellName, cwd);
    const parsed = parseShellDiscoveryOutput(output);
    return {
      executable: shellExecutable,
      shellName,
      supported: true,
      loaded: true,
      builtins: parsed.builtins,
      aliases: parsed.aliases,
      functions: parsed.functions,
    };
  } catch {
    return {
      executable: shellExecutable,
      shellName,
      supported: true,
      loaded: false,
      builtins: [],
      aliases: [],
      functions: [],
    };
  }
}

// 将 shell 快照摊平成与 PATH 扫描一致的 `DiscoveredCommand[]` 结构，
// 这样 service 层可以把两类来源统一合并处理。
export function flattenShellCommands(snapshot: ShellDiscoverySnapshot): DiscoveredCommand[] {
  if (!snapshot.loaded) {
    return [];
  }
  return [
    ...snapshot.builtins.map(
      (name): DiscoveredCommand => ({
        name,
        source: 'shell_builtin',
        kind: 'builtin',
      }),
    ),
    ...snapshot.aliases.map(
      (alias): DiscoveredCommand => ({
        name: alias.name,
        source: 'shell_alias',
        kind: 'alias',
        detail: alias.expansion,
      }),
    ),
    ...snapshot.functions.map(
      (name): DiscoveredCommand => ({
        name,
        source: 'shell_function',
        kind: 'function',
      }),
    ),
  ];
}

// 空快照用于表达“没有执行 shell 探测”或“当前 shell 不支持”的状态，
// 而不是简单返回空数组；这样上层还能给出可解释的 notes。
export function createEmptyShellSnapshot(executable: string | null): ShellDiscoverySnapshot {
  return {
    executable,
    shellName: executable ? detectShellName(executable) : null,
    supported: false,
    loaded: false,
    builtins: [],
    aliases: [],
    functions: [],
  };
}

function detectShellName(shellExecutable: string) {
  const fileName = basename(shellExecutable).toLowerCase();
  return fileName.endsWith('.exe') ? fileName.slice(0, -4) : fileName;
}

// 通过启动一个登录交互 shell 把 builtin/alias/function 分段打印出来。
// zsh 与 bash 采用不同命令模板，但最终都输出同一套带标记的文本协议，
// 方便后续统一解析。
async function runShellDiscovery(
  shellExecutable: string,
  shellName: string,
  cwd: string,
): Promise<string> {
  const command =
    shellName === 'zsh'
      ? [
          'emulate -L zsh',
          "print -r -- '__LANIA_BUILTINS__'",
          'print -l -- ${(ok)builtins}',
          "print -r -- '__LANIA_ALIASES__'",
          'alias -L',
          "print -r -- '__LANIA_FUNCTIONS__'",
          'print -l -- ${(ok)functions}',
        ].join('; ')
      : [
          "printf '%s\\n' '__LANIA_BUILTINS__'",
          'compgen -b',
          "printf '%s\\n' '__LANIA_ALIASES__'",
          'alias -p',
          "printf '%s\\n' '__LANIA_FUNCTIONS__'",
          "while read -r _ _ name; do printf '%s\\n' \"$name\"; done < <(declare -F)",
        ].join('; ');

  return new Promise((resolvePromise, reject) => {
    const child = spawn(shellExecutable, ['-lic', command], {
      cwd,
      env: process.env,
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
      if (exitCode === 0) {
        resolvePromise(stdout);
        return;
      }
      reject(new Error(stderr || stdout || `shell discovery exited with code ${String(exitCode)}`));
    });
  });
}

// 把 shell 输出解析成结构化快照。
// 这里依赖我们在 `runShellDiscovery` 中定义的分段哨兵标记，
// 因此新增 shell 支持时，需要同时维护输出协议与解析逻辑。
export function parseShellDiscoveryOutput(
  output: string,
): Omit<ShellDiscoverySnapshot, 'executable' | 'shellName' | 'supported' | 'loaded'> {
  const builtins = new Set<string>();
  const aliases = new Map<string, string>();
  const functions = new Set<string>();
  let section: 'builtins' | 'aliases' | 'functions' | null = null;

  for (const rawLine of output.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }
    if (line === '__LANIA_BUILTINS__') {
      section = 'builtins';
      continue;
    }
    if (line === '__LANIA_ALIASES__') {
      section = 'aliases';
      continue;
    }
    if (line === '__LANIA_FUNCTIONS__') {
      section = 'functions';
      continue;
    }
    if (section === 'builtins') {
      builtins.add(line);
      continue;
    }
    if (section === 'functions') {
      functions.add(line);
      continue;
    }
    if (section === 'aliases') {
      const parsed = parseAliasLine(line);
      if (parsed) {
        aliases.set(parsed.name, parsed.expansion);
      }
    }
  }

  return {
    builtins: [...builtins].sort(),
    aliases: [...aliases.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([name, expansion]) => ({ name, expansion })),
    functions: [...functions].sort(),
  };
}

// alias 输出在 bash/zsh 中都接近 `name=value`，但可能带引号包装。
// 这里把行解析与引号剥离集中处理，避免主解析流程被字符串细节淹没。
function parseAliasLine(line: string): { name: string; expansion: string } | null {
  const normalized = line.startsWith('alias ') ? line.slice(6) : line;
  const separator = normalized.indexOf('=');
  if (separator <= 0) {
    return null;
  }
  const name = normalized.slice(0, separator).trim();
  const rawExpansion = normalized.slice(separator + 1).trim();
  if (!name || !rawExpansion) {
    return null;
  }
  return {
    name,
    expansion: stripWrappingQuotes(rawExpansion),
  };
}

function stripWrappingQuotes(value: string) {
  if (value.length >= 2) {
    const first = value[0];
    const last = value[value.length - 1];
    if ((first === "'" && last === "'") || (first === '"' && last === '"')) {
      return value.slice(1, -1);
    }
  }
  return value;
}
