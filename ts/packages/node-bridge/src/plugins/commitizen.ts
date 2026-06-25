/**
 * commitizen 插件，定位配置并返回可执行命令计划。
 *
 * 主要导出：commitizenPlugin。
 */
import type { BridgeEvent } from '../protocol/events.js';
import { asRecord, loadToolConfig } from '../core/runtime.js';

export const commitizenPlugin = {
  name: 'commitizen',
  methods: ['commitizen.run'],
  async handle(
    method: string,
    params: Record<string, unknown>,
    context?: { cwd: string | null },
  ) {
    if (method !== 'commitizen.run') {
      return null;
    }

    const cwd =
      typeof params.cwd === 'string'
        ? params.cwd
        : context?.cwd ?? null;
    const loadedConfig =
      cwd ? await loadToolConfig(cwd, 'commitizen') : { configPath: null, config: {}, exists: false };
    const commitizenConfig = normalizeCommitizenConfig(loadedConfig.config);
    let kind = typeof params.kind === 'string' ? params.kind : 'chore';
    const scope = typeof params.scope === 'string' ? params.scope : null;
    let subject =
      typeof params.subject === 'string' ? params.subject : 'sync changes';
    const warnings: string[] = [];

    if (commitizenConfig.types.length > 0 && !commitizenConfig.types.includes(kind)) {
      kind = commitizenConfig.types[0];
      warnings.push(`Commit type was adjusted to "${kind}" from commitizen config`);
    }
    if (commitizenConfig.subjectLimit && subject.length > commitizenConfig.subjectLimit) {
      subject = subject.slice(0, commitizenConfig.subjectLimit).trimEnd();
      warnings.push(
        `Commit subject was trimmed to ${commitizenConfig.subjectLimit} characters from commitizen config`,
      );
    }

    const message = scope
      ? `${kind}(${scope}): ${subject}`
      : `${kind}: ${subject}`;

    return {
      result: {
        accepted: true,
        tool: 'commitizen',
        message,
        kind,
        scope,
        subject,
        configLoaded: loadedConfig.exists,
        configPath: loadedConfig.configPath,
      },
      events: [
        {
          method: 'event.log',
          params: {
            level: 'info',
            message: loadedConfig.configPath
              ? `Commitizen workflow prepared using ${loadedConfig.configPath}`
              : 'Commitizen workflow prepared',
          },
        } satisfies BridgeEvent,
        ...warnings.map(
          (message) =>
            ({
              method: 'event.log',
              params: {
                level: 'warn',
                message,
              },
            }) satisfies BridgeEvent,
        ),
      ],
    };
  },
};

function normalizeCommitizenConfig(config: Record<string, unknown>): {
  types: string[];
  subjectLimit: number | null;
} {
  const record = asRecord(config);
  const types = Array.isArray(record.types)
    ? record.types
        .map((entry) => {
          if (typeof entry === 'string') {
            return entry;
          }
          const candidate = asRecord(entry).value;
          return typeof candidate === 'string' ? candidate : null;
        })
        .filter((entry): entry is string => Boolean(entry))
    : [];
  const subjectLimit = typeof record.subjectLimit === 'number' && record.subjectLimit > 0
    ? record.subjectLimit
    : null;
  return { types, subjectLimit };
}
