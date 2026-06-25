/**
 * 负责执行 dynamic command / scaffold 在真正落盘前的环境检查。
 *
 * 这里的 guard 更偏“稳定、可解释的启发式判断”，而不是完整环境建模：
 * - Node 版本范围是否满足
 * - 某个命令是否能从 PATH 找到
 * - 当前项目更像单包仓还是 monorepo
 */
import { access, constants } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

type WorkspaceKind = 'single' | 'monorepo';

interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
}

type VersionComparator =
  | { operator: '>' | '>=' | '<' | '<=' | '='; version: ParsedVersion }
  | { operator: 'range'; min: ParsedVersion; maxExclusive: ParsedVersion };

export function evaluateNodeVersionRange(versionText: string, range: string): {
  ok: boolean;
  normalizedVersion: string | null;
  normalizedRange: string;
  reason?: string;
} {
  // 这里只支持一组够用的 semver 子集，用来满足 guard 判断。
  // 如果表达式超出支持范围，会明确返回 unsupported，而不是给出不可靠结论。
  const version = parseVersion(versionText);
  if (!version) {
    return {
      ok: false,
      normalizedVersion: null,
      normalizedRange: normalizeWhitespace(range),
      reason: `unable to parse node version from \`${versionText}\``,
    };
  }

  const normalizedRange = normalizeWhitespace(range);
  const groups = parseVersionRange(normalizedRange);
  if (groups.length === 0) {
    return {
      ok: false,
      normalizedVersion: formatVersion(version),
      normalizedRange,
      reason: `unsupported node version range \`${range}\``,
    };
  }

  const ok = groups.some((group) => group.every((entry) => compareVersion(version, entry)));
  return {
    ok,
    normalizedVersion: formatVersion(version),
    normalizedRange,
    reason: ok ? undefined : `expected ${normalizedRange}, received ${formatVersion(version)}`,
  };
}

export async function detectCommandOnPath(
  command: string,
  envPath: string | undefined,
): Promise<{ ok: boolean; resolvedPath: string | null; reason?: string }> {
  // 直接按 PATH 展开候选路径，避免依赖 shell 的 `which/where`，这样跨平台行为更可控。
  if (!command) {
    return { ok: false, resolvedPath: null, reason: 'command name is empty' };
  }

  const candidates = createCommandCandidates(command, envPath);
  for (const candidate of candidates) {
    try {
      await access(candidate, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
      return {
        ok: true,
        resolvedPath: candidate,
      };
    } catch {
      continue;
    }
  }

  return {
    ok: false,
    resolvedPath: null,
    reason: `command \`${command}\` was not found in PATH`,
  };
}

export async function detectWorkspaceKind(input: {
  packageJson: Record<string, unknown> | null;
  hasFile: (filePath: string) => Promise<boolean>;
}): Promise<{
  kind: WorkspaceKind;
  indicators: string[];
}> {
  // 多个常见 monorepo 信号只要命中任意一个，就认为当前仓库更接近 monorepo。
  // 同时返回 indicators，方便后续生成更可解释的失败信息。
  const indicators: string[] = [];
  const workspaces = readWorkspaceGlobs(input.packageJson);
  if (workspaces.length > 0) {
    indicators.push('package.json#workspaces');
  }
  if (await input.hasFile('pnpm-workspace.yaml')) {
    indicators.push('pnpm-workspace.yaml');
  }
  if (await input.hasFile('lerna.json')) {
    indicators.push('lerna.json');
  }
  if (await input.hasFile('rush.json')) {
    indicators.push('rush.json');
  }

  return {
    kind: indicators.length > 0 ? 'monorepo' : 'single',
    indicators,
  };
}

export function formatGuardFailureMessage(result: Record<string, unknown>): string {
  const name = String(result.name ?? result.type ?? 'guard');
  const detail = typeof result.message === 'string' && result.message.length > 0
    ? result.message
    : defaultGuardFailureMessage(result);
  return `${name}: ${detail}`;
}

function defaultGuardFailureMessage(result: Record<string, unknown>): string {
  switch (result.type) {
    case 'directory_empty':
      return `directory is not empty (${joinStrings(readStringArray(result.entries))})`;
    case 'node_version':
      return `node version check failed (expected ${String(result.range ?? 'unknown')}, actual ${String(
        result.version ?? 'unknown',
      )})`;
    case 'command_exists':
      return `required command \`${String(result.command ?? 'unknown')}\` is unavailable`;
    case 'workspace_kind':
      return `workspace kind mismatch (expected ${String(result.expected ?? 'unknown')}, actual ${String(
        result.actual ?? 'unknown',
      )})`;
    default:
      return 'guard check failed';
  }
}

function readWorkspaceGlobs(packageJson: Record<string, unknown> | null): string[] {
  // 同时兼容 `workspaces: []` 与 `workspaces: { packages: [] }` 两种常见写法。
  if (!packageJson) {
    return [];
  }
  if (Array.isArray(packageJson.workspaces)) {
    return packageJson.workspaces.filter((entry): entry is string => typeof entry === 'string');
  }
  if (
    packageJson.workspaces &&
    typeof packageJson.workspaces === 'object' &&
    Array.isArray((packageJson.workspaces as Record<string, unknown>).packages)
  ) {
    return ((packageJson.workspaces as Record<string, unknown>).packages as unknown[]).filter(
      (entry): entry is string => typeof entry === 'string',
    );
  }
  return [];
}

function createCommandCandidates(command: string, envPath: string | undefined): string[] {
  // Windows 上命令可执行性还依赖 PATHEXT，因此需要补齐一组扩展名候选。
  const pathEntries = (envPath ?? '').split(path.delimiter).filter(Boolean);
  if (path.isAbsolute(command) || command.includes(path.sep)) {
    return [command];
  }

  const windowsExtensions =
    process.platform === 'win32'
      ? (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM')
          .split(';')
          .map((entry) => entry.trim())
          .filter(Boolean)
      : [''];

  const candidates: string[] = [];
  for (const entry of pathEntries) {
    for (const extension of windowsExtensions) {
      candidates.push(path.join(entry, `${command}${extension}`));
    }
  }
  return candidates;
}

function parseVersionRange(range: string): VersionComparator[][] {
  // `||` 表示“任意一组命中即可”；组内多个 comparator 之间则是“全部满足”关系。
  return range
    .split('||')
    .map((segment) => segment.trim())
    .filter(Boolean)
    .map((segment) =>
      segment
        .split(/\s+/)
        .map((token) => parseComparator(token))
        .filter((token): token is VersionComparator => Boolean(token)),
    )
    .filter((group) => group.length > 0);
}

function parseComparator(token: string): VersionComparator | null {
  const normalized = token.trim();
  if (!normalized) {
    return null;
  }

  const comparatorMatch = normalized.match(/^(>=|<=|>|<|=)?(.+)$/);
  if (!comparatorMatch) {
    return null;
  }

  const operator = comparatorMatch[1] as '>' | '>=' | '<' | '<=' | '=' | undefined;
  const rawVersion = comparatorMatch[2]?.trim() ?? '';
  if (!rawVersion) {
    return null;
  }

  const explicit = parseExplicitVersion(rawVersion);
  if (!explicit) {
    return null;
  }

  if (operator) {
    return { operator, version: explicit.version };
  }

  // 未显式写比较符时，按常见 semver 习惯补成范围：
  // - `18` => `>=18.0.0 <19.0.0`
  // - `18.17` => `>=18.17.0 <18.18.0`
  // - `18.17.1` => 精确匹配
  if (explicit.precision === 1) {
    return {
      operator: 'range',
      min: explicit.version,
      maxExclusive: { major: explicit.version.major + 1, minor: 0, patch: 0 },
    };
  }
  if (explicit.precision === 2) {
    return {
      operator: 'range',
      min: explicit.version,
      maxExclusive: { major: explicit.version.major, minor: explicit.version.minor + 1, patch: 0 },
    };
  }
  return { operator: '=', version: explicit.version };
}

function parseVersion(versionText: string): ParsedVersion | null {
  const parsed = parseExplicitVersion(versionText.trim());
  return parsed?.version ?? null;
}

function parseExplicitVersion(
  value: string,
): { version: ParsedVersion; precision: 1 | 2 | 3 } | null {
  const normalized = value.trim().replace(/^v/i, '');
  const match = normalized.match(/^(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:[-+].*)?$/);
  if (!match) {
    return null;
  }

  const precision = match[3] ? 3 : match[2] ? 2 : 1;
  return {
    version: {
      major: Number(match[1] ?? 0),
      minor: Number(match[2] ?? 0),
      patch: Number(match[3] ?? 0),
    },
    precision,
  };
}

function compareVersion(version: ParsedVersion, comparator: VersionComparator): boolean {
  if (comparator.operator === 'range') {
    return (
      compareTriples(version, comparator.min) >= 0 &&
      compareTriples(version, comparator.maxExclusive) < 0
    );
  }

  const compared = compareTriples(version, comparator.version);
  switch (comparator.operator) {
    case '>':
      return compared > 0;
    case '>=':
      return compared >= 0;
    case '<':
      return compared < 0;
    case '<=':
      return compared <= 0;
    case '=':
      return compared === 0;
    default:
      return false;
  }
}

function compareTriples(left: ParsedVersion, right: ParsedVersion): number {
  if (left.major !== right.major) {
    return left.major - right.major;
  }
  if (left.minor !== right.minor) {
    return left.minor - right.minor;
  }
  return left.patch - right.patch;
}

function formatVersion(version: ParsedVersion): string {
  return `${version.major}.${version.minor}.${version.patch}`;
}

function normalizeWhitespace(value: string): string {
  return value.trim().replace(/\s+/g, ' ');
}

function readStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === 'string') : [];
}

function joinStrings(values: string[]): string {
  return values.length > 0 ? values.join(', ') : 'none';
}
