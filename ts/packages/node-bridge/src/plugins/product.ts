import { handleBuild } from './product/handlers/build.js';
import { handleGenerate } from './product/handlers/generate.js';
import { handleInspect } from './product/handlers/inspect.js';
import { handlePack } from './product/handlers/pack.js';
import { handlePublish } from './product/handlers/publish.js';

// Product plugin entrypoint.
//
// This file intentionally stays small:
// - It wires method names to handler implementations.
// - All real logic lives in `./product/**` so individual concerns remain under ~700 lines.

export const productPlugin = {
  name: 'product',
  methods: [
    'product.generate',
    'product.inspect',
    'product.build',
    'product.pack',
    'product.publish',
  ],
  async handle(method: string, params: Record<string, unknown>) {
    switch (method) {
      case 'product.generate':
        return await handleGenerate(params);
      case 'product.inspect':
        return await handleInspect(params);
      case 'product.build':
        return await handleBuild(params);
      case 'product.pack':
        return await handlePack(params);
      case 'product.publish':
        return await handlePublish(params);
      default:
        return null;
    }
  },
};

