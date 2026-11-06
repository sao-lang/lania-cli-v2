/**
 * Node 生态模板（toolkit / toolkit-monorepo / spa-react / spa-vue / express / nestjs）
 * 共用的包管理器 / lint / 测试选项。
 *
 * 唯一来源是运行时工具链表（`presentation.ts`，设计文档 B14）：
 * 这里不再保留选项字面量，新增/调整选项只改 `RUNTIME_PRESENTATIONS.node`。
 */
import type { ChoiceSpec } from './language-options.js';
import { choicesFor } from './presentation.js';

export const NODE_CHOICES: ChoiceSpec = choicesFor('node');
