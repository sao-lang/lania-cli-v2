/**
 * Node bridge 请求分发入口，处理握手、内建方法与插件调用。
 *
 * 主要导出：createHandshakeResponse、handleRequest、handleExchange、readyEvent。
 */
export { createHandshakeResponse, handleRequest, handleExchange, readyEvent } from './entry/index.js';
export type { SchemaTools } from './core/schema-tools.js';
