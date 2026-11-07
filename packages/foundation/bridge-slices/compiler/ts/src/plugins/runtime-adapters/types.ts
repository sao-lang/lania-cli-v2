import type { BridgeEvent } from '@lania-cli-v2/bridge-runtime/protocol/events.js';
import type {
  CompilerAdapterContext,
  CompilerAdapterParams,
  CompilerExecution,
  Runtime,
} from '@lania-cli-v2/bridge-slice-compiler/plugins/compiler-shared.js';

export type SpawnRuntime = Exclude<Runtime, 'web'>;

export interface RuntimeAdapter {
  runtime: SpawnRuntime;
  handleDev(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
    emit?: (event: BridgeEvent) => void,
  ): Promise<CompilerExecution | null>;
  handleBuild(
    params: CompilerAdapterParams,
    context: CompilerAdapterContext,
    emit?: (event: BridgeEvent) => void,
  ): Promise<CompilerExecution | null>;
}
