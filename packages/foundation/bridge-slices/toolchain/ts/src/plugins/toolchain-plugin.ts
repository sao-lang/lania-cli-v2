/**
 * `toolchain.describe` —— 把运行时知识表暴露给 Rust 宿主。
 *
 * 为什么要做成 bridge 方法而不是 Rust 侧硬编码：
 * 见 `docs/design/multi-runtime-bridge.zh-CN.md` §5.2 —— 同一份知识只允许一处定义，
 * Rust / 各语言 bridge / `lan doctor` 全部从这一个方法读取。
 */
import { describeToolchains } from './toolchain.js';

export const toolchainPlugin = {
  name: 'toolchain',
  methods: ['toolchain.describe'],
  async handle(method: string) {
    if (method !== 'toolchain.describe') {
      return null;
    }
    return {
      result: describeToolchains(),
      events: [],
    };
  },
};
