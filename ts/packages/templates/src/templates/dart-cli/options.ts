import {
  normalizeChoices,
  type ChoiceSpec,
  type NormalizedChoices,
} from '../../language-options.js';
import { choicesFor } from '../../presentation.js';

/**
 * Dart 项目的包管理器 / lint / 测试选项来自运行时工具链表（设计文档 B14）：
 * `RUNTIME_PRESENTATIONS.dart` 是唯一来源，这里只做取值。
 */
export const DART_CHOICES: ChoiceSpec = choicesFor('dart');

export interface DartCliOptions extends NormalizedChoices {
  packageName: string;
}

export function normalizeDartCliOptions(
  options: Record<string, unknown> = {},
): DartCliOptions {
  const packageName =
    typeof options.packageName === 'string' && options.packageName.length > 0
      ? options.packageName
      : 'demo';
  return { packageName, ...normalizeChoices(options, DART_CHOICES) };
}
