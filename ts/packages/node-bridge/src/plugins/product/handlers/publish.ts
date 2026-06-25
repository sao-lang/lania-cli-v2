import { fileExists } from '../../../core/runtime.js';
import type { ProductPublishManifest } from '../types.js';
import {
  collectResumedSteps,
  createInitialExecutionState,
  createRetryPolicy,
  executePublishManifest,
  persistPublishArtifacts,
} from '../publish/execution.js';
import { normalizePublishOptions, createExecutionRequest, createInitialExecution } from '../publish/options.js';
import { preparePublishArtifacts } from '../publish/prepare.js';
import { readJsonFile } from '../fs.js';

// `product.publish` assembles a publish-ready npm package layout, emits a manifest,
// and optionally executes the plan (npm publish) with retry/rollback.

export async function handlePublish(params: Record<string, unknown>) {
  const options = normalizePublishOptions(params);
  const { manifest, manifestPath, report, reportPath } = await preparePublishArtifacts(params, options);

  let previousManifest: ProductPublishManifest | null = null;
  if (options.resume) {
    if (options.clean) {
      throw new Error('product.publish --resume requires --no-clean so prior manifest state is preserved');
    }
    if (await fileExists(manifestPath)) {
      previousManifest = (await readJsonFile(manifestPath)) as unknown as ProductPublishManifest;
    }
  }

  manifest.execution = createInitialExecution(
    createInitialExecutionState,
    createRetryPolicy,
    options,
    collectResumedSteps(previousManifest, manifest),
  );

  await persistPublishArtifacts(manifestPath, reportPath, manifest, report);

  if (options.executePublish) {
    manifest.execution = await executePublishManifest(
      manifest,
      report,
      createExecutionRequest(manifestPath, reportPath, options),
    );
  }

  return {
    result: report,
    events: [],
  };
}
