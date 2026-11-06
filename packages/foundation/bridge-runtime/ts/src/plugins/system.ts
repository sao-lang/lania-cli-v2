import type { BridgePlugin } from '../core/bridge-plugin.js';
import type { BridgeEvent } from '../protocol/events.js';
import { normalizeOptions } from './system/options.js';
import { discoverSystemCommands } from './system/service.js';
import { parseShellDiscoveryOutput } from './system/shell-discovery.js';

export { parseShellDiscoveryOutput } from './system/shell-discovery.js';

export const systemPlugin: BridgePlugin = {
  name: 'system',
  methods: ['system.listCommands'],
  async handle(method, params, context) {
    if (method !== 'system.listCommands') {
      return null;
    }

    const options = normalizeOptions(params, context?.cwd ?? undefined);
    const result = await discoverSystemCommands(options);

    return {
      result,
      events: [
        logEvent(
          `Scanned ${result.summary.scannedDirs} PATH directories and matched ${result.summary.matched} terminal command(s)`,
        ),
      ],
    };
  },
};

function logEvent(message: string): BridgeEvent {
  return {
    method: 'event.log',
    params: {
      level: 'info',
      message,
    },
  };
}
