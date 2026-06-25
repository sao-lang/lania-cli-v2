import { asRecord } from '../../core/runtime.js';
import type {
  PromptMapFunctionSpec,
  PromptOnAnsweredActionSpec,
  PromptSpec,
  PromptValidationSpec,
  PromptWhenSpec,
} from './types.js';
import { stringArray } from './parse-shared.js';

/**
 * 解析 manifest 中声明的 prompt 数组。
 * 这里集中处理兼容字段和弱校验，避免主解析流程继续膨胀。
 */
export function parsePrompt(value: unknown): PromptSpec[] {
  if (typeof value === 'function') {
    try {
      return parsePrompt((value as () => unknown)());
    } catch {
      return [];
    }
  }
  if (!Array.isArray(value)) {
    return [];
  }

  const results: PromptSpec[] = [];
  for (const item of value) {
    const prompt = asRecord(item);
    const field = typeof prompt.field === 'string' ? prompt.field : '';
    const message = parseLocalizedText(prompt.message);
    if (!field || !message) {
      continue;
    }

    const spec: PromptSpec = {
      message,
      field,
      whenMissing: stringArray(prompt.whenMissing, []),
      defaultValue: prompt.defaultValue,
    };
    if (typeof prompt.id === 'string') {
      spec.id = prompt.id;
    }

    const kind =
      typeof prompt.kind === 'string'
        ? prompt.kind
        : typeof prompt.type === 'string'
          ? prompt.type
          : null;
    if (
      kind === 'input' ||
      kind === 'select' ||
      kind === 'confirm' ||
      kind === 'multi_select' ||
      kind === 'password' ||
      kind === 'editor' ||
      kind === 'number' ||
      kind === 'fuzzy_select' ||
      kind === 'autocomplete' ||
      kind === 'search' ||
      kind === 'rawlist' ||
      kind === 'expand'
    ) {
      spec.kind = kind;
    }

    const detail = parseLocalizedText(prompt.detail);
    if (detail) {
      spec.detail = detail;
    }

    if (Array.isArray(prompt.choices)) {
      const choices = prompt.choices
        .map((choice) => {
          const record = asRecord(choice);
          return typeof record.label === 'string'
            ? { label: record.label, value: record.value }
            : null;
        })
        .filter((choice): choice is { label: string; value: unknown } => choice !== null);
      if (choices.length > 0) {
        spec.choices = choices;
      }
    }

    const when = normalizePromptWhen(prompt.when);
    if (when) {
      spec.when = when;
    }
    if (typeof prompt.goto === 'string' && prompt.goto) {
      spec.goto = prompt.goto;
    }

    const validate = normalizePromptValidate(prompt.validate);
    if (validate.length > 0) {
      spec.validate = validate;
    }
    if (typeof prompt.timeoutMs === 'number' && Number.isFinite(prompt.timeoutMs)) {
      spec.timeoutMs = prompt.timeoutMs;
    }
    if (typeof prompt.contextKey === 'string' && prompt.contextKey) {
      spec.contextKey = prompt.contextKey;
    }
    if (prompt.accumulation === 'replace' || prompt.accumulation === 'append') {
      spec.accumulation = prompt.accumulation;
    }
    if (typeof prompt.returnable === 'boolean') {
      spec.returnable = prompt.returnable;
    }

    const mapFunctions = normalizePromptMapFunctions(prompt.mapFunctions);
    if (mapFunctions.length > 0) {
      spec.mapFunctions = mapFunctions;
    }

    const onAnswered = normalizePromptOnAnswered(prompt.onAnswered);
    if (onAnswered.length > 0) {
      spec.onAnswered = onAnswered;
    }

    results.push(spec);
  }

  return results;
}

function parseLocalizedText(value: unknown): PromptSpec['message'] | undefined {
  if (typeof value === 'string' && value) {
    return value;
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }

  const record = value as Record<string, unknown>;
  const localized = Object.fromEntries(
    Object.entries(record).filter(([, item]) => typeof item === 'string'),
  ) as PromptSpec['message'];
  return Object.keys(localized).length > 0 ? localized : undefined;
}

function normalizePromptWhen(value: unknown): PromptWhenSpec | undefined {
  const record = asRecord(value);
  const type = typeof record.type === 'string' ? record.type : '';
  const key = typeof record.key === 'string' ? record.key : '';
  if (!type || !key) {
    return undefined;
  }
  if (type === 'equals' || type === 'not_equals') {
    return { type, key, value: record.value };
  }
  if (type === 'exists' || type === 'truthy') {
    return { type, key };
  }
  return undefined;
}

function normalizePromptValidate(value: unknown): PromptValidationSpec[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry): PromptValidationSpec | null => {
      if (entry === 'required') {
        return 'required';
      }

      const record = asRecord(entry);
      const type = typeof record.type === 'string' ? record.type : '';
      if (type === 'required') {
        return { type };
      }
      if (type === 'min_length') {
        const min =
          typeof record.min === 'number'
            ? record.min
            : typeof record.value === 'number'
              ? record.value
              : undefined;
        return typeof min === 'number' ? { type, min } : null;
      }
      if (type === 'one_of') {
        const values = stringArray(record.values ?? record.choices, []);
        return values.length > 0 ? { type, values } : null;
      }
      return null;
    })
    .filter((entry): entry is PromptValidationSpec => entry !== null);
}

function normalizePromptMapFunctions(value: unknown): PromptMapFunctionSpec[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => {
      if (
        entry === 'trim' ||
        entry === 'lowercase' ||
        entry === 'uppercase' ||
        entry === 'to_number' ||
        entry === 'json_parse'
      ) {
        return entry;
      }

      const record = asRecord(entry);
      const type = typeof record.type === 'string' ? record.type : '';
      if (
        type === 'trim' ||
        type === 'lowercase' ||
        type === 'uppercase' ||
        type === 'to_number' ||
        type === 'json_parse'
      ) {
        return { type } as const;
      }
      if (type === 'split' && typeof record.separator === 'string') {
        return { type, separator: record.separator } as const;
      }
      return null;
    })
    .filter((entry): entry is PromptMapFunctionSpec => entry !== null);
}

function normalizePromptOnAnswered(value: unknown): PromptOnAnsweredActionSpec[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => {
      const record = asRecord(entry);
      const type = typeof record.type === 'string' ? record.type : '';
      if (type === 'set_context_value' && typeof record.key === 'string') {
        return { type, key: record.key, value: record.value } as const;
      }
      if (type === 'set_context_from_answer' && typeof record.key === 'string') {
        const action: PromptOnAnsweredActionSpec = { type, key: record.key };
        if (typeof record.field === 'string') {
          action.field = record.field;
        }
        const mapFunctions = normalizePromptMapFunctions(record.mapFunctions);
        if (mapFunctions.length > 0) {
          action.mapFunctions = mapFunctions;
        }
        return action;
      }
      if (type === 'goto' && typeof record.target === 'string') {
        return { type, target: record.target } as const;
      }
      if (type === 'goto_if' && typeof record.target === 'string') {
        const when = normalizePromptWhen(record.when);
        return when ? { type, target: record.target, when } : null;
      }
      return null;
    })
    .filter((entry): entry is PromptOnAnsweredActionSpec => entry !== null);
}
