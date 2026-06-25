/**
 * commitlint 插件，解析配置并返回 lint 命令与结果。
 *
 * 主要导出：commitlintPlugin。
 */
import type { BridgeEvent } from '../protocol/events.js';
import { asRecord, loadToolConfig } from '../core/runtime.js';

export const commitlintPlugin = {
  name: 'commitlint',
  methods: ['commitlint.run'],
  async handle(
    method: string,
    params: Record<string, unknown>,
    context?: { cwd: string | null },
  ) {
    if (method !== 'commitlint.run') {
      return null;
    }

    const message = typeof params.message === 'string' ? params.message : '';
    const cwd =
      typeof params.cwd === 'string'
        ? params.cwd
        : context?.cwd ?? null;
    const loadedConfig =
      cwd ? await loadToolConfig(cwd, 'commitlint') : { configPath: null, config: {}, exists: false };
    const validation = validateCommitMessage(message, loadedConfig.config);

    return {
      result: {
        accepted: true,
        tool: 'commitlint',
        valid: validation.valid,
        message,
        configLoaded: loadedConfig.exists,
        configPath: loadedConfig.configPath,
        errors: validation.errors,
      },
      events: buildEvents(loadedConfig.configPath, validation.valid, validation.errors),
    };
  },
};

function buildEvents(
  configPath: string | null,
  valid: boolean,
  errors: string[],
): BridgeEvent[] {
  const events: BridgeEvent[] = [
    {
      method: 'event.log',
      params: {
        level: 'info',
        message: configPath
          ? `Commitlint check completed using ${configPath}`
          : 'Commitlint check completed',
      },
    },
  ];
  for (const error of errors) {
    events.push({
      method: 'event.log',
      params: {
        level: 'warn',
        message: error,
      },
    });
  }
  if (!valid && errors.length === 0) {
    events.push({
      method: 'event.log',
      params: {
        level: 'warn',
        message: 'Commit message does not satisfy commitlint validation rules',
      },
    });
  }
  return events;
}

function validateCommitMessage(
  message: string,
  config: Record<string, unknown>,
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  const parsed = parseCommitMessage(message);
  if (!parsed.matchesConventional) {
    errors.push('commit message must match "<type>(<scope>): <subject>" or "<type>: <subject>"');
  }

  const rules = asRecord(config.rules);
  validatePresenceRule(errors, 'type-empty', parsed.type, 'type');
  validatePresenceRule(errors, 'subject-empty', parsed.subject, 'subject');
  validateEnumRule(errors, parsed.type, 'type-enum', 'type');
  validateEnumRule(errors, parsed.scope, 'scope-enum', 'scope');
  validateScopeRule(errors, parsed.scope, rules['scope-empty']);
  validateHeaderLength(errors, parsed.header, rules['header-max-length']);

  return {
    valid: errors.length === 0,
    errors,
  };

  function validatePresenceRule(
    target: string[],
    ruleName: string,
    value: string | null,
    label: string,
  ) {
    const rule = asRuleTuple(rules[ruleName]);
    if (!isEnabled(rule)) {
      return;
    }
    const isEmpty = !value;
    const condition = rule[1];
    if (condition === 'never' && isEmpty) {
      target.push(`${label} must not be empty`);
    } else if (condition === 'always' && !isEmpty) {
      target.push(`${label} must be empty`);
    }
  }

  function validateEnumRule(
    target: string[],
    value: string | null,
    ruleName: string,
    label: string,
  ) {
    const rule = asRuleTuple(rules[ruleName]);
    if (!isEnabled(rule) || !value) {
      return;
    }
    const candidates = Array.isArray(rule[2])
      ? rule[2].filter((entry): entry is string => typeof entry === 'string')
      : [];
    if (candidates.length === 0) {
      return;
    }
    const condition = rule[1];
    const included = candidates.includes(value);
    if (condition === 'always' && !included) {
      target.push(`${label} must be one of: ${candidates.join(', ')}`);
    } else if (condition === 'never' && included) {
      target.push(`${label} must not be one of: ${candidates.join(', ')}`);
    }
  }
}

function validateScopeRule(
  errors: string[],
  scope: string | null,
  rawRule: unknown,
) {
  const rule = asRuleTuple(rawRule);
  if (!isEnabled(rule)) {
    return;
  }
  const empty = !scope;
  const condition = rule[1];
  if (condition === 'never' && empty) {
    errors.push('scope must not be empty');
  } else if (condition === 'always' && !empty) {
    errors.push('scope must be empty');
  }
}

function validateHeaderLength(
  errors: string[],
  header: string,
  rawRule: unknown,
) {
  const rule = asRuleTuple(rawRule);
  if (!isEnabled(rule)) {
    return;
  }
  const maxLength = typeof rule[2] === 'number' ? rule[2] : null;
  if (!maxLength) {
    return;
  }
  const condition = rule[1];
  const withinLimit = header.length <= maxLength;
  if (condition === 'always' && !withinLimit) {
    errors.push(`header must not be longer than ${maxLength} characters`);
  } else if (condition === 'never' && withinLimit) {
    errors.push(`header must be longer than ${maxLength} characters`);
  }
}

function parseCommitMessage(message: string): {
  header: string;
  type: string | null;
  scope: string | null;
  subject: string | null;
  matchesConventional: boolean;
} {
  const header = message.trim();
  const match = /^(?<type>[a-z0-9._-]+)(?:\((?<scope>[^)]+)\))?: (?<subject>.+)$/i.exec(header);
  return {
    header,
    type: match?.groups?.type ?? null,
    scope: match?.groups?.scope ?? null,
    subject: match?.groups?.subject ?? null,
    matchesConventional: Boolean(match),
  };
}

function asRuleTuple(value: unknown): [number, string, unknown?] | null {
  return Array.isArray(value) &&
    typeof value[0] === 'number' &&
    typeof value[1] === 'string'
    ? [value[0], value[1], value[2]]
    : null;
}

function isEnabled(rule: [number, string, unknown?] | null): rule is [number, string, unknown?] {
  return Array.isArray(rule) && rule[0] > 0;
}
