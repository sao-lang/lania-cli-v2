/**
 * 生成器清单桥接（§4.5.6）：把 Rust 侧的 `Generator` 表暴露给 TS 调用方。
 *
 * 为什么 TS 不做自己的清单：
 * 生成器表在仓库内**只有一处**定义（`lania-workflows/src/create/add/generators.rs`）。
 * 这里只做协议转发与形状归一化，任何新增生成器都不需要改 TS。
 */
import type { BridgeEvent } from '../../protocol/events.js';
import { hostCall } from '../../core/host-rpc.js';
import { asRecord } from '../../core/runtime.js';

export interface GeneratorQuestionPayload {
  field: string;
  label: string;
  labelZh?: string;
  kind: 'input' | 'select' | 'confirm';
  required: boolean;
  choices: string[];
}

export interface GeneratorPayload {
  name: string;
  framework: string;
  label: string;
  labelZh?: string;
  wantsName: boolean;
  artifactCount: number;
  questions: GeneratorQuestionPayload[];
}

export interface GeneratorCatalog {
  framework: string | null;
  frameworkReason: string;
  generators: GeneratorPayload[];
}

export async function listGeneratorsResult(cwd: string | null) {
  const root = cwd ?? process.cwd();
  // `addGenerator.list` 的权威实现在宿主（Rust）：这里只传 cwd。
  // host RPC 不可用（例如脱离宿主独立运行 bridge）时返回空清单，而不是伪造一份。
  try {
    const exchange = await hostCall<Record<string, unknown>>('addGenerator.list', { cwd: root });
    return {
      result: normalizeCatalog(asRecord(exchange.result)),
      events: (Array.isArray(exchange.events) ? exchange.events : []) as BridgeEvent[],
    };
  } catch (error) {
    return {
      result: {
        framework: null,
        frameworkReason: `host rpc unavailable: ${error instanceof Error ? error.message : String(error)}`,
        generators: [],
      } satisfies GeneratorCatalog,
      events: [] as BridgeEvent[],
    };
  }
}

function normalizeCatalog(raw: Record<string, unknown>): GeneratorCatalog {
  const generators = Array.isArray(raw.generators) ? raw.generators : [];
  return {
    framework: typeof raw.framework === 'string' && raw.framework.length > 0 ? raw.framework : null,
    frameworkReason: typeof raw.frameworkReason === 'string' ? raw.frameworkReason : 'unknown',
    generators: generators.map((value) => {
      const record = asRecord(value);
      return {
        name: String(record.name ?? ''),
        framework: String(record.framework ?? ''),
        label: String(record.label ?? record.name ?? ''),
        labelZh: typeof record.labelZh === 'string' ? record.labelZh : undefined,
        wantsName: record.wantsName !== false,
        artifactCount: Number(record.artifactCount ?? 0),
        questions: (Array.isArray(record.questions) ? record.questions : []).map((question) => {
          const entry = asRecord(question);
          return {
            field: String(entry.field ?? ''),
            label: String(entry.label ?? entry.field ?? ''),
            labelZh: typeof entry.labelZh === 'string' ? entry.labelZh : undefined,
            kind:
              entry.kind === 'select' || entry.kind === 'confirm' ? entry.kind : ('input' as const),
            required: entry.required === true,
            choices: Array.isArray(entry.choices)
              ? entry.choices.filter((choice): choice is string => typeof choice === 'string')
              : [],
          };
        }),
      };
    }),
  };
}
