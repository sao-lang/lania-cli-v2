/**
 * bridge 运行指标计数器，用于 request、heartbeat 与插件错误统计。
 *
 * 主要导出：createBridgeMetrics、BridgeRuntimeMetrics。
 *
 * 这些指标是“进程级”累积值：
 * - 生命周期从 bridge 进程启动开始，到进程退出结束
 * - `bridge.metrics` 会读取一个 snapshot，供 Rust 侧观测当前桥接状态
 */
export interface BridgeRuntimeMetrics {
  /** 收到的 request 数量（包括 bridge 内建方法与插件方法）。 */
  requests: number;
  /** 已发出的事件总数（按 event 条数累计，而不是按 request 次数累计）。 */
  events: number;
  /** 动态插件成功加载次数。 */
  pluginLoads: number;
  /** 动态插件被拒绝次数（声明非法 / policy 拒绝 / 导出不合法 等）。 */
  pluginRejects: number;
  /** 插件执行期抛错次数。 */
  pluginErrors: number;
  /** bridge.heartbeat 被调用次数。 */
  heartbeats: number;
}

export function createBridgeMetrics() {
  const metrics: BridgeRuntimeMetrics = {
    requests: 0,
    events: 0,
    pluginLoads: 0,
    pluginRejects: 0,
    pluginErrors: 0,
    heartbeats: 0,
  };

  return {
    metrics,
    /** 每收到一条 request 就调用一次。 */
    recordRequest() {
      metrics.requests += 1;
    },
    /** 记录本次请求产生的事件数量。 */
    recordEvents(count: number) {
      metrics.events += count;
    },
    recordPluginLoad() {
      metrics.pluginLoads += 1;
    },
    recordPluginReject() {
      metrics.pluginRejects += 1;
    },
    recordPluginError() {
      metrics.pluginErrors += 1;
    },
    recordHeartbeat() {
      metrics.heartbeats += 1;
    },
    /** 返回当前快照，避免把内部可变对象直接暴露给外部。 */
    snapshot(): BridgeRuntimeMetrics {
      return { ...metrics };
    },
  };
}
