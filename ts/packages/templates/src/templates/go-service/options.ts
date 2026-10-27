import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';
import { choicesFor } from '../../presentation.js';

/**
 * Go 项目的 lint / 测试选项来自运行时工具链表（设计文档 B14）：
 * `RUNTIME_PRESENTATIONS.go` 是唯一来源，这里只做取值。
 */
export const GO_CHOICES: ChoiceSpec = choicesFor('go');

export interface GoServiceOptions extends NormalizedChoices {
  modulePath: string;
}

export function normalizeGoServiceOptions(
  options: Record<string, unknown> = {},
): GoServiceOptions {
  const modulePath =
    typeof options.modulePath === 'string' && options.modulePath.length > 0
      ? options.modulePath
      : 'example.com/demo';
  return { modulePath, ...normalizeChoices(options, GO_CHOICES) };
}
