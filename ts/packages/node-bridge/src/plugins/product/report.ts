import { PRODUCT_REPORT_VERSION } from './constants.js';
import type { ProductDistributionReport } from './types.js';
import { uniqueStrings } from './utils.js';

// Helpers for creating the persisted JSON reports produced by build/pack/publish.

export function createDistributionReport(
  input: Omit<
    ProductDistributionReport,
    'accepted' | 'reportVersion' | 'generatedFiles' | 'exitCode'
  > & {
    generatedFiles: string[];
  },
  reportFileName: 'build-report.json' | 'pack-report.json' | 'publish-report.json',
): ProductDistributionReport {
  return {
    accepted: true,
    reportVersion: PRODUCT_REPORT_VERSION,
    kind: input.kind,
    mode: input.mode,
    outputRoot: input.outputRoot,
    productRoot: input.productRoot,
    nodeBridgeDir: input.nodeBridgeDir,
    wrapper: input.wrapper,
    tarball: input.tarball,
    bundle: input.bundle,
    checks: input.checks,
    generatedFiles: uniqueStrings([...input.generatedFiles, `./${reportFileName}`]),
    ...(input.experimental ? { experimental: input.experimental } : {}),
    exitCode: 0,
  };
}

