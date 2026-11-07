// Keep the historical entry file so the suite aggregator can continue importing
// `./service-tools.js` while the actual cases live in smaller themed modules.
export { registerTests } from './service-tools/index.js';
