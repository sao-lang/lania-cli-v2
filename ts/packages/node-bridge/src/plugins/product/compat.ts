import { join } from 'node:path';

import { asRecord } from '../../core/runtime.js';

import { packageRoot } from './constants.js';
import { readJsonFile } from './fs.js';
import type {
  ProductCompatDeclared,
  ProductCompatSnapshot,
  ProductCompatSummary,
} from './types.js';

// Compatibility checks exist to catch drift between:
// - host CLI version
// - node-bridge version
// - protocol version
// - product package version / declared ranges
//
// We intentionally keep the range matcher tiny and dependency-free. It only supports
// the subset we need in configs (exact, ^, ~, and AND ranges like ">=1.0.0 <2.0.0").

function parseVersion(value: string): [number, number, number] | null {
  const raw = value.trim().replace(/^v/i, '');
  const match = raw.match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) {
    return null;
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function compareVersions(a: [number, number, number], b: [number, number, number]): number {
  for (let i = 0; i < 3; i += 1) {
    if (a[i] > b[i]) return 1;
    if (a[i] < b[i]) return -1;
  }
  return 0;
}

function versionSatisfies(version: string, range: string): boolean {
  const parsed = parseVersion(version);
  if (!parsed) {
    return false;
  }
  const raw = range.trim();
  if (raw === '' || raw === '*' || raw.toLowerCase() === 'any') {
    return true;
  }

  // Support simple whitespace AND ranges: ">=1.0.0 <2.0.0"
  const parts = raw.split(/\s+/).filter(Boolean);
  if (parts.length > 1) {
    return parts.every((part) => versionSatisfies(version, part));
  }

  const part = parts[0] ?? raw;
  if (part.startsWith('^') || part.startsWith('~')) {
    // ^1.2.3 means "same major, >=base"
    // ~1.2.3 means "same major+minor, >=base"
    const base = parseVersion(part.slice(1));
    if (!base) return false;
    const cmp = compareVersions(parsed, base);
    if (cmp < 0) return false;
    if (part.startsWith('^')) {
      return parsed[0] === base[0];
    }
    return parsed[0] === base[0] && parsed[1] === base[1];
  }

  for (const op of ['>=', '<=', '>', '<'] as const) {
    if (part.startsWith(op)) {
      const base = parseVersion(part.slice(op.length));
      if (!base) return false;
      const cmp = compareVersions(parsed, base);
      if (op === '>=') return cmp >= 0;
      if (op === '<=') return cmp <= 0;
      if (op === '>') return cmp > 0;
      return cmp < 0;
    }
  }

  // Exact match
  const exact = parseVersion(part);
  if (!exact) return false;
  return compareVersions(parsed, exact) === 0;
}

export async function computeProductCompatSnapshot(input: {
  productConfig: Record<string, unknown>;
  packageJson: Record<string, unknown>;
  hostVersion: string | null;
}): Promise<ProductCompatSnapshot> {
  // Read node-bridge version from its local package.json (this is what we stage into bundles).
  const nodeBridgePackage = await readJsonFile(join(packageRoot, 'package.json'));
  const nodeBridgeVersion =
    typeof nodeBridgePackage.version === 'string' ? nodeBridgePackage.version : null;

  // NOTE: protocolVersion is currently a constant; once the protocol evolves this should be
  // sourced from a single authoritative location.
  const protocolVersion = '0.1.0';

  const declaredCompat = asRecord(input.productConfig.compat);
  const declared: ProductCompatDeclared = {
    frameworkVersionRange:
      typeof declaredCompat.frameworkVersionRange === 'string'
        ? declaredCompat.frameworkVersionRange
        : null,
    protocolVersionRange:
      typeof declaredCompat.protocolVersionRange === 'string'
        ? declaredCompat.protocolVersionRange
        : null,
    nodeBridgeVersionRange:
      typeof declaredCompat.nodeBridgeVersionRange === 'string'
        ? declaredCompat.nodeBridgeVersionRange
        : null,
    productVersionRange:
      typeof declaredCompat.productVersionRange === 'string'
        ? declaredCompat.productVersionRange
        : null,
  };

  const reasons: string[] = [];
  let verdict: ProductCompatSummary['verdict'] = 'ready';

  // Missing info is treated as warn: it usually means the caller didn't provide enough context.
  if (!input.hostVersion) {
    verdict = 'warn';
    reasons.push('missing hostVersion in bridge request params');
  }
  if (typeof input.packageJson.version !== 'string' || input.packageJson.version.trim().length == 0) {
    verdict = 'warn';
    reasons.push('missing product package.json version');
  }

  // Range checks: any mismatch becomes warn, but we keep collecting reasons for better UX.
  if (declared.frameworkVersionRange && input.hostVersion) {
    if (!versionSatisfies(input.hostVersion, declared.frameworkVersionRange)) {
      verdict = 'warn';
      reasons.push(
        `hostVersion ${input.hostVersion} does not satisfy frameworkVersionRange ${declared.frameworkVersionRange}`,
      );
    }
  }
  if (declared.protocolVersionRange) {
    if (!versionSatisfies(protocolVersion, declared.protocolVersionRange)) {
      verdict = 'warn';
      reasons.push(
        `protocolVersion ${protocolVersion} does not satisfy protocolVersionRange ${declared.protocolVersionRange}`,
      );
    }
  }
  if (declared.nodeBridgeVersionRange && nodeBridgeVersion) {
    if (!versionSatisfies(nodeBridgeVersion, declared.nodeBridgeVersionRange)) {
      verdict = 'warn';
      reasons.push(
        `nodeBridgeVersion ${nodeBridgeVersion} does not satisfy nodeBridgeVersionRange ${declared.nodeBridgeVersionRange}`,
      );
    }
  }
  if (declared.productVersionRange && typeof input.packageJson.version === 'string') {
    if (!versionSatisfies(input.packageJson.version, declared.productVersionRange)) {
      verdict = 'warn';
      reasons.push(
        `productVersion ${input.packageJson.version} does not satisfy productVersionRange ${declared.productVersionRange}`,
      );
    }
  }

  return {
    verdict,
    reasons,
    declared,
    actual: {
      hostVersion: input.hostVersion,
      protocolVersion,
      nodeBridgeName: '@lania-cli/node-bridge',
      nodeBridgeVersion,
    },
    product: {
      packageName: typeof input.packageJson.name === 'string' ? input.packageJson.name : null,
      productName: typeof input.productConfig.name === 'string' ? input.productConfig.name : null,
      productBinaryName:
        typeof input.productConfig.binaryName === 'string' ? input.productConfig.binaryName : null,
      productVersion: typeof input.packageJson.version === 'string' ? input.packageJson.version : null,
    },
  };
}

