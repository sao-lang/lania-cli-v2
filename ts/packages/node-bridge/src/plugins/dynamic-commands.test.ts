// Keep a single `.test.ts` entry so Node's test glob discovers this suite once,
// while the actual cases live in grouped modules next to a shared fixture file.
import { registerTests as registerBridgeToolTests } from './dynamic-commands-test/bridge-tools.js';
import { registerTests as registerHostToolTests } from './dynamic-commands-test/host-tools.js';
import { registerTests as registerInlineHookTests } from './dynamic-commands-test/inline-hooks.js';
import { registerTests as registerInvokeTests } from './dynamic-commands-test/invoke.js';
import { registerTests as registerResolveTests } from './dynamic-commands-test/resolve.js';
import { registerTests as registerServiceToolTests } from './dynamic-commands-test/service-tools.js';

registerResolveTests();
registerInvokeTests();
registerInlineHookTests();
registerBridgeToolTests();
registerHostToolTests();
registerServiceToolTests();
