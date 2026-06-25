// Shared type definitions for the product plugin.
//
// Rationale:
// - `product.ts` was historically a single giant file that mixed types + handlers + helpers.
// - Splitting handlers into modules requires a stable "types hub" to avoid circular imports.

export type ProductReportKind = 'product_build' | 'product_pack' | 'product_publish';
export type ProductReportMode = 'snapshot' | 'install_root' | 'npm_package';
export type ProductReportChecks = Record<string, boolean>;

export type ProductBundlePlatformSource =
  | 'official_staging'
  | 'request'
  | 'environment'
  | 'package_dir'
  | 'optional_dependency';
export type ProductBundlePlatformStatus = 'ready' | 'binary_missing' | 'package_missing';

export interface ProductBundlePlatformTarball {
  packageName: string;
  platform: string;
  version: string;
  tarball: string;
  source: ProductBundlePlatformSource;
}

export interface ProductBundlePlatformMatrixEntry {
  packageName: string;
  platform: string;
  version: string;
  status: ProductBundlePlatformStatus;
  tarball: string | null;
  source: ProductBundlePlatformSource;
}

export interface ProductPublishBundle {
  root: string;
  cliTarball: string | null;
  platformTarball: string | null;
  platformTarballs: ProductBundlePlatformTarball[];
  platformMatrix: ProductBundlePlatformMatrixEntry[];
}

export interface ProductPublishManifestPackage {
  role: 'product' | 'official_cli' | 'platform_binary';
  name: string;
  version: string;
  tarball: string;
  distTag: string;
  channel: string;
  publishStrategy: 'npm_tarball_publish';
  optional?: boolean;
  platform?: string;
  source?: ProductBundlePlatformTarball['source'];
}

export interface ProductPublishManifestDependencyLink {
  from: string;
  to: string;
  type: 'dependency' | 'optional_dependency';
  field: 'dependencies' | 'optionalDependencies';
}

export interface ProductPublishManifestStep {
  id: string;
  packageName: string;
  role: ProductPublishManifestPackage['role'];
  tarball: string;
  distTag: string;
  dependsOn: string[];
  publishConfig: {
    registry: string;
    access: 'public';
    otpRequired: 'unknown';
    provenance: false;
    dryRun: false;
  };
  command: {
    program: 'npm';
    args: string[];
  };
}

export interface ProductPublishAttempt {
  stepId: string;
  packageName: string;
  attempt: number;
  status: 'succeeded' | 'failed';
  retriable: boolean;
  startedAt: string;
  finishedAt: string;
  args: string[];
  error: string | null;
}

export interface ProductPublishRollbackCommand {
  stepId: string;
  packageName: string;
  version: string | null;
  registry: string;
  command: string[];
}

export interface ProductPublishManifest {
  kind: 'product_publish_manifest';
  mode: 'registry_plan';
  outputRoot: string;
  packageName: string;
  packageVersion: string;
  binaryName: string;
  distTag: string;
  channel: string;
  productTarball: string;
  bundleRoot: string | null;
  packages: ProductPublishManifestPackage[];
  platformMatrix: ProductBundlePlatformMatrixEntry[];
  publishOrder: string[];
  dependencyLinks: ProductPublishManifestDependencyLink[];
  steps: ProductPublishManifestStep[];
  checks: ProductReportChecks;
  execution?: {
    executed: boolean;
    dryRun: boolean;
    completedSteps: string[];
    resumed: boolean;
    failedStepId: string | null;
    lastError: string | null;
    attempts: ProductPublishAttempt[];
    retryPolicy: {
      maxRetries: number;
      retryDelayMs: number;
    };
    rollbackPlan: {
      status: 'not_needed' | 'planned' | 'executed' | 'failed';
      generatedAt: string | null;
      reason: string | null;
      commands: ProductPublishRollbackCommand[];
    };
    updatedAt: string;
    preflight?: {
      checked: boolean;
      actor: string | null;
      registry: string;
      tarballsVerified: number;
      versionConflicts: string[];
    };
  };
}

export interface ProductDistributionReport {
  accepted: true;
  reportVersion: number;
  kind: ProductReportKind;
  mode: ProductReportMode;
  outputRoot: string;
  productRoot: string | null;
  nodeBridgeDir: string | null;
  wrapper: string | null;
  tarball: string | null;
  bundle: ProductPublishBundle | null;
  checks: ProductReportChecks;
  generatedFiles: string[];
  experimental?: Record<string, unknown>;
  exitCode: 0;
}

export interface ProductGenerateReport {
  accepted: true;
  reportVersion: number;
  kind: 'product_generate';
  mode: 'scaffold';
  outputRoot: string;
  checks: Record<string, boolean>;
  generatedFiles: string[];
  experimental?: Record<string, unknown>;
  exitCode: 0;
}

export interface ProductCompatSummary {
  verdict: 'ready' | 'warn';
  reasons: string[];
  reportPath: string;
  declared?: ProductCompatDeclared;
  actual: {
    hostVersion: string | null;
    protocolVersion: string;
    nodeBridgeName: string;
    nodeBridgeVersion: string | null;
  };
  product: {
    packageName: string | null;
    productName: string | null;
    productBinaryName: string | null;
    productVersion: string | null;
  };
}

export interface ProductInspectReport {
  accepted: true;
  reportVersion: number;
  kind: 'product_inspect';
  mode: 'development' | 'installed';
  doctor?: boolean;
  cwd: string;
  configPath: string | null;
  checks: Record<string, boolean>;
  compat?: ProductCompatSummary;
  product: {
    name: string | null;
    binaryName: string | null;
    displayName: string | null;
    templatesDir: string | null;
  };
  schema: {
    entries: string[];
    roots: string[];
    warnings: string[];
  };
  artifacts: {
    buildDir: string;
    hasBuildReport: boolean;
    packDir: string;
    hasPackReport: boolean;
    publishDir: string;
    hasPublishReport: boolean;
  };
  packageJson: {
    name: string | null;
    version: string | null;
  };
  nextSteps: string[];
  exitCode: 0;
}

export interface ProductCompatDeclared {
  frameworkVersionRange: string | null;
  protocolVersionRange: string | null;
  nodeBridgeVersionRange: string | null;
  productVersionRange: string | null;
}

export interface ProductCompatSnapshot {
  verdict: ProductCompatSummary['verdict'];
  reasons: string[];
  declared: ProductCompatDeclared;
  actual: ProductCompatSummary['actual'];
  product: ProductCompatSummary['product'];
}

export interface ResolvedPlatformBinarySource {
  packageName: string;
  packageRoot: string;
  platform: string;
  binaryPath: string;
  source: ProductBundlePlatformSource;
}

export interface OfficialPlatformPackageDescriptor {
  packageName: string;
  packageRoot: string;
  platform: string;
  version: string;
  binaryPath: string;
  packageExists: boolean;
}

