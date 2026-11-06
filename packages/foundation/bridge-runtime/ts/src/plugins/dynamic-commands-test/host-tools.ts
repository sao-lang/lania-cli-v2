// Keep the historical entry file so the suite aggregator can continue importing
// `./host-tools.js` while the actual cases live in smaller themed modules.
export { registerTests } from './host-tools/index.js';
