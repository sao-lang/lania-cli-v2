/**
 * `tools.json`：围绕 JSON 文本和 JSON 文件的轻量工具层。
 *
 * 这层故意很薄，只提供 parse/stringify/read/write/patch。
 * 适合 schema 在处理结构化配置时复用，不需要再手写重复的 JSON 读写样板。
 */
import { readFile, writeFile } from 'node:fs/promises';

export interface JsonTools {
  parse: (text: string) => unknown;
  stringify: (value: unknown, space?: number) => string;
  read: (filePath: string) => Promise<unknown>;
  write: (filePath: string, value: unknown, options?: { space?: number }) => Promise<void>;
  patch: (
    filePath: string,
    updater: (value: any) => any,
    options?: { space?: number },
  ) => Promise<void>;
}

export function createJsonTools(policy: {
  assertJsonAllowed: (operation: string) => Promise<void>;
}): JsonTools {
  return {
    parse: (text) => JSON.parse(text),
    stringify: (value, space) => JSON.stringify(value, null, typeof space === 'number' ? space : 2),
    read: async (filePath) => {
      await policy.assertJsonAllowed('read');
      return JSON.parse(await readFile(filePath, 'utf8'));
    },
    write: async (filePath, value, options) => {
      await policy.assertJsonAllowed('write');
      const space = typeof options?.space === 'number' ? options.space : 2;
      await writeFile(filePath, JSON.stringify(value, null, space));
    },
    patch: async (filePath, updater, options) => {
      await policy.assertJsonAllowed('patch');
      // `patch()` 的约定是“先读整个 JSON，再交给 updater 返回下一份完整值”，
      // 它不做字段级 diff，也不尝试保留原始格式细节。
      const space = typeof options?.space === 'number' ? options.space : 2;
      const current = JSON.parse(await readFile(filePath, 'utf8'));
      const next = updater(current);
      await writeFile(filePath, JSON.stringify(next, null, space));
    },
  };
}
