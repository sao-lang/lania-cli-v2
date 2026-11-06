import { getEnvApiKey, getModel } from '@earendil-works/pi-ai/compat';
import type { Model } from '@earendil-works/pi-ai';

export interface ResolvedModel {
  model: Model<string>;
  apiKey: string;
}

/**
 * 解析模型与凭据。
 *
 * 优先级：
 * 1. `LANIA_AGENT_BASE_URL`：构造一个 OpenAI 兼容模型（本地 stub / 自建网关）。
 * 2. `LANIA_AGENT_PROVIDER` + `LANIA_AGENT_MODEL`：pi 内置目录。
 * 3. 默认 `anthropic` + `claude-sonnet-4-5`。
 */
export function resolveModel(providerArg?: string, modelArg?: string): ResolvedModel {
  const baseUrl = process.env.LANIA_AGENT_BASE_URL?.trim();
  const apiKey = process.env.LANIA_AGENT_API_KEY?.trim();
  const provider = providerArg?.trim() || process.env.LANIA_AGENT_PROVIDER?.trim() || 'anthropic';
  const modelId = modelArg?.trim() || process.env.LANIA_AGENT_MODEL?.trim() || 'claude-sonnet-4-5';

  if (baseUrl) {
    const model: Model<'openai-completions'> = {
      id: modelId,
      name: modelId,
      api: 'openai-completions',
      provider,
      baseUrl,
      input: ['text'],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128_000,
      maxTokens: 8_192,
      reasoning: false,
    };
    return { model, apiKey: apiKey || 'local' };
  }

  // pi 的目录函数把 provider 收紧成 KnownProvider 联合；这里 provider 来自用户输入，
  // 未知值由目录查询返回 undefined 兜底，因此在边界处放宽类型。
  const lookupModel = getModel as unknown as (
    provider: string,
    modelId: string,
  ) => Model<string> | undefined;
  const lookupEnvKey = getEnvApiKey as unknown as (provider: string) => string | undefined;

  let model: Model<string> | undefined;
  try {
    model = lookupModel(provider, modelId);
  } catch (error) {
    throw new Error(
      `unknown model "${provider}/${modelId}": ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (!model) {
    throw new Error(`unknown model "${provider}/${modelId}"`);
  }

  const resolvedKey = apiKey || lookupEnvKey(provider);
  if (!resolvedKey) {
    throw new Error(
      `missing API key for provider "${provider}" (set ${provider.toUpperCase().replace(/-/g, '_')}_API_KEY)`,
    );
  }
  return { model, apiKey: resolvedKey };
}
