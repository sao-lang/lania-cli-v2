import type { BridgePlugin } from '../../core/bridge-plugin.js';
import { invokeDynamicCommand } from './execute.js';
import { resolveDynamicCommands } from './parse.js';

/**
 * Dynamic Commands Plugin
 *
 * 这个插件负责把“项目运行时命令”动态挂载进 CLI（Rust 侧），并把命令执行回调到 Node 侧。
 *
 * 两阶段流程（Rust <-> Node）：
 * 1) `commands.resolveDynamic`：Node 解析 manifest，产出命令树和 handler 注册表。
 * 2) `command.invokeDynamic`：Rust 回传 target/argv，Node 根据 target 执行本地函数或插件方法。
 */
export const dynamicCommandsPlugin: BridgePlugin = {
  name: 'dynamic-commands',
  methods: ['commands.resolveDynamic', 'command.invokeDynamic'],
  async handle(method, params, context) {
    const cwd = (typeof params.cwd === 'string' ? params.cwd : context?.cwd) ?? process.cwd();

    switch (method) {
      case 'commands.resolveDynamic':
        return resolveDynamicCommands(cwd, params);
      case 'command.invokeDynamic':
        return invokeDynamicCommand(cwd, params);
      default:
        return null;
    }
  },
};
