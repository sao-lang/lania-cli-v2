/**
 * `tools.interaction`：与宿主交互式输入能力的 schema facade。
 *
 * 它覆盖两类场景：
 * - 单次提问：input/confirm/select/password/editor
 * - 多步骤交互流：prompt/createFlow
 *
 * 这一层的重点在于把问题描述、上下文、fallback 和恢复状态统一整理成
 * host 能消费的稳定协议，而不是在 Node 侧自己实现交互 UI。
 */
import { asRecord } from '../core/runtime.js';
import { createHostInvoker } from './host-utils.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

type InteractionChoice = {
  label: string;
  value: unknown;
};

type InteractionFallback =
  | { type: 'use_default' | 'use_defaults' }
  | { type: 'use_value'; value: unknown }
  | { type: 'skip' }
  | { type: 'error' };

export type InteractionQuestion = {
  id?: string;
  field?: string;
  name?: string;
  message: string | Record<string, string>;
  detail?: string | Record<string, string>;
  choices?: InteractionChoice[];
  defaultValue?: unknown;
  when?: Record<string, unknown>;
  goto?: string;
  validate?: unknown[];
  timeoutMs?: number;
  contextKey?: string;
  accumulation?: 'replace' | 'append';
  returnable?: boolean;
  mapFunctions?: unknown[];
  onAnswered?: unknown[];
};

export type InteractionPromptOptions = {
  answers?: Record<string, unknown>;
  context?: Record<string, unknown>;
  accumulate?: boolean;
  resetAccumulated?: boolean;
  fallback?: InteractionFallback;
  locale?: string;
  resumeFrom?: Record<string, unknown>;
};

export type InteractionPromptState = {
  current_step_id?: string | null;
  answers: Record<string, unknown>;
  context: Record<string, unknown>;
  completed_steps: string[];
  timed_out_steps: string[];
  interrupted: boolean;
};

export interface InteractionFlow {
  addQuestion: (question: InteractionQuestion) => InteractionFlow;
  addQuestions: (questions: InteractionQuestion[]) => InteractionFlow;
  insertQuestions: (
    questions: InteractionQuestion[],
    options: { beforeId?: string; afterId?: string },
  ) => InteractionFlow;
  updateContext: (ctx: Record<string, unknown>) => InteractionFlow;
  resetAccumulated: () => Promise<void>;
  execute: (options?: InteractionPromptOptions) => Promise<InteractionPromptState>;
}

export interface InteractionTools {
  input: (options: InteractionQuestion & InteractionPromptOptions) => Promise<unknown>;
  confirm: (options: InteractionQuestion & InteractionPromptOptions) => Promise<boolean>;
  select: (options: InteractionQuestion & InteractionPromptOptions) => Promise<unknown>;
  multiSelect: (options: InteractionQuestion & InteractionPromptOptions) => Promise<unknown[]>;
  password: (options: InteractionQuestion & InteractionPromptOptions) => Promise<string>;
  editor: (options: InteractionQuestion & InteractionPromptOptions) => Promise<string>;
  prompt: (
    questionOrQuestions: InteractionQuestion | InteractionQuestion[],
    options?: InteractionPromptOptions,
  ) => Promise<InteractionPromptState>;
  createFlow: (options?: {
    questions?: InteractionQuestion[];
    context?: Record<string, unknown>;
    locale?: string;
  }) => InteractionFlow;
}

export function createInteractionTools(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
): InteractionTools {
  const host = createHostInvoker(base);
  const askSingle = async (
    method: string,
    options: InteractionQuestion & InteractionPromptOptions,
  ) => {
    // 单问题入口全部统一走 `host.interaction.<method>`，只在这里收口策略校验和参数归一化。
    await policy.assertInteractionAllowed(method);
    const result = await host.call<{ answer: unknown }>(`host.interaction.${method}`, {
      ...normalizeInteractionQuestion(options),
      ...normalizeInteractionPromptOptions(options, base),
    });
    return result.answer;
  };

  const prompt = async (
    questionOrQuestions: InteractionQuestion | InteractionQuestion[],
    options?: InteractionPromptOptions,
  ): Promise<InteractionPromptState> => {
    // `prompt()` 是“多题一次执行”的快捷入口；更复杂的增删题逻辑留给 flow。
    await policy.assertInteractionAllowed('prompt');
    const questions = Array.isArray(questionOrQuestions)
      ? questionOrQuestions
      : [questionOrQuestions];
    const result = await host.call<InteractionPromptState>('host.interaction.prompt', {
      questions: questions.map(normalizeInteractionQuestion),
      ...normalizeInteractionPromptOptions(options, base),
    });
    return normalizeInteractionPromptState(result);
  };

  return {
    input: (options) => askSingle('input', options),
    confirm: async (options) => Boolean(await askSingle('confirm', options)),
    select: (options) => askSingle('select', options),
    multiSelect: async (options) => {
      const answer = await askSingle('multiSelect', options);
      return Array.isArray(answer) ? answer : [];
    },
    password: async (options) => String((await askSingle('password', options)) ?? ''),
    editor: async (options) => String((await askSingle('editor', options)) ?? ''),
    prompt,
    createFlow: (options) => createInteractionFlow(base, policy, host, options),
  };
}

function createInteractionFlow(
  base: SchemaToolContext,
  policy: ToolsPolicyManager,
  host: ReturnType<typeof createHostInvoker>,
  options?: {
    questions?: InteractionQuestion[];
    context?: Record<string, unknown>;
    locale?: string;
  },
): InteractionFlow {
  // flow 是一个纯内存构建器：真正执行前只维护 questions/context，
  // 到 `execute()` 时才一次性发给 host。
  const questions = [...(options?.questions ?? [])];
  let context = { ...(options?.context ?? {}) };
  const locale = options?.locale;

  const api: InteractionFlow = {
    addQuestion: (question) => {
      questions.push(question);
      return api;
    },
    addQuestions: (nextQuestions) => {
      questions.push(...nextQuestions);
      return api;
    },
    insertQuestions: (nextQuestions, insertOptions) => {
      const beforeId = insertOptions.beforeId;
      const afterId = insertOptions.afterId;
      if (beforeId) {
        const index = questions.findIndex(
          (question) => interactionQuestionId(question) === beforeId,
        );
        if (index >= 0) {
          questions.splice(index, 0, ...nextQuestions);
          return api;
        }
      }
      if (afterId) {
        const index = questions.findIndex(
          (question) => interactionQuestionId(question) === afterId,
        );
        if (index >= 0) {
          questions.splice(index + 1, 0, ...nextQuestions);
          return api;
        }
      }
      questions.push(...nextQuestions);
      return api;
    },
    updateContext: (nextContext) => {
      context = { ...context, ...nextContext };
      return api;
    },
    resetAccumulated: async () => {
      await policy.assertInteractionAllowed('resetAccumulated');
      await host.call('host.interaction.resetAccumulated');
    },
    execute: async (runOptions) => {
      await policy.assertInteractionAllowed('flow.execute');
      // 运行时 context 以“flow 默认 context + 本次 execute 覆盖”的方式合并。
      const result = await host.call<InteractionPromptState>('host.interaction.flow.execute', {
        questions: questions.map(normalizeInteractionQuestion),
        ...normalizeInteractionPromptOptions(
          {
            ...runOptions,
            context: { ...context, ...(runOptions?.context ?? {}) },
            locale: runOptions?.locale ?? locale,
          },
          base,
        ),
      });
      return normalizeInteractionPromptState(result);
    },
  };

  return api;
}

function interactionQuestionId(question: InteractionQuestion): string | undefined {
  return question.id ?? question.field ?? question.name;
}

function normalizeInteractionQuestion(question: InteractionQuestion): Record<string, unknown> {
  // 统一补齐 id/field，避免 host 侧再为 `id/field/name` 三套写法做分支兼容。
  const normalized: Record<string, unknown> = {
    ...question,
    id: question.id ?? question.field ?? question.name ?? 'value',
    field: question.field ?? question.name ?? question.id ?? 'value',
  };
  if (question.name !== undefined && question.field === undefined) {
    delete normalized.name;
  }
  return normalized;
}

function normalizeInteractionPromptOptions(
  options: InteractionPromptOptions | undefined,
  base: SchemaToolContext,
): Record<string, unknown> {
  // 这里顺手把 cwd 一起注入，让 host 能知道当前交互归属于哪个工作区上下文。
  return {
    answers: options?.answers ?? {},
    context: options?.context ?? {},
    accumulate: options?.accumulate ?? false,
    resetAccumulated: options?.resetAccumulated ?? false,
    fallback: options?.fallback,
    locale: options?.locale,
    resumeFrom: options?.resumeFrom,
    cwd: base.cwd,
  };
}

function toStringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : [];
}

function normalizeInteractionPromptState(
  state: InteractionPromptState | Record<string, unknown> | null | undefined,
): InteractionPromptState {
  // host 侧字段目前同时兼容 snake_case / camelCase，这里统一归一成 schema 层固定形状。
  const record = asRecord(state ?? {});
  return {
    current_step_id:
      typeof record.current_step_id === 'string' || record.current_step_id === null
        ? (record.current_step_id as string | null)
        : typeof record.currentStepId === 'string' || record.currentStepId === null
          ? (record.currentStepId as string | null)
          : null,
    answers: asRecord(record.answers),
    context: asRecord(record.context),
    completed_steps: toStringArray(record.completed_steps ?? record.completedSteps),
    timed_out_steps: toStringArray(record.timed_out_steps ?? record.timedOutSteps),
    interrupted: Boolean(record.interrupted),
  };
}
