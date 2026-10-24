import type { BridgeEvent } from '../../protocol/events.js';
import type {
  CompilerAdapterContext,
  CompilerAdapterParams,
  CompilerExecution,
  Runtime,
} from '../compiler-shared.js';

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
