import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';
import { choicesFor } from '../../presentation.js';

/**
 * Rust 项目的 lint / 测试选项来自运行时工具链表（设计文档 B14）：
 * `RUNTIME_PRESENTATIONS.rust` 是唯一来源，这里只做取值。
 */
export const RUST_CHOICES: ChoiceSpec = choicesFor('rust');

export interface RustLibOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeRustLibOptions(
  options: Record<string, unknown> = {},
): RustLibOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName, ...normalizeChoices(options, RUST_CHOICES) };
}
