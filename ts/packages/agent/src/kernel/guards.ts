/** 运行时形状收窄：pi 交给工具的入参、pi 的运行时消息都按 unknown 处理。 */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
