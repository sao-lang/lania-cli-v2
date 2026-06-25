import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

// NOTE:
// This module centralizes "where am I" and other constants shared by the product plugin.
// Keeping these in one file avoids subtle drift when different handlers compute roots differently.

export const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
// `ts/packages/node-bridge` -> repository root (`lania-cli-v2`)
export const repoRoot = resolve(packageRoot, '..', '..', '..');

// Report schema versions (bumped when we make breaking changes to persisted JSON outputs).
export const PRODUCT_REPORT_VERSION = 1;
export const COMPAT_REPORT_VERSION = 1;

// For publish flows we need a stable default platform package to fall back to (useful in dev).
export const OFFICIAL_DARWIN_ARM64_PACKAGE_NAME = '@lania-cli/cli-darwin-arm64';
