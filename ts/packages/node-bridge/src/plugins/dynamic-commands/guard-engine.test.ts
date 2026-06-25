import assert from 'node:assert/strict';
import { chmod, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  detectCommandOnPath,
  detectWorkspaceKind,
  evaluateNodeVersionRange,
  formatGuardFailureMessage,
} from './guard-engine.js';

test('evaluateNodeVersionRange supports comparator and shorthand ranges', () => {
  assert.equal(evaluateNodeVersionRange('v18.19.1', '>=18').ok, true);
  assert.equal(evaluateNodeVersionRange('18.19.1', '18').ok, true);
  assert.equal(evaluateNodeVersionRange('18.19.1', '18.19').ok, true);
  assert.equal(evaluateNodeVersionRange('18.19.1', '>=18 <19').ok, true);
  assert.equal(evaluateNodeVersionRange('20.0.0', '>=18 <20').ok, false);
  assert.equal(evaluateNodeVersionRange('garbled', '>=18').ok, false);
});

test('detectCommandOnPath resolves executables from PATH', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-guard-path-'));
  t.after(async () => await rm(root, { recursive: true, force: true }));

  const commandPath = path.join(root, 'demo-command');
  await writeFile(commandPath, '#!/bin/sh\nexit 0\n', 'utf8');
  await chmod(commandPath, 0o755);

  const detected = await detectCommandOnPath('demo-command', root);
  assert.equal(detected.ok, true);
  assert.equal(detected.resolvedPath, commandPath);

  const missing = await detectCommandOnPath('missing-command', root);
  assert.equal(missing.ok, false);
  assert.match(String(missing.reason), /not found in PATH/);
});

test('detectWorkspaceKind recognizes package.json and workspace marker files', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-guard-workspace-'));
  t.after(async () => await rm(root, { recursive: true, force: true }));

  const fromPackageJson = await detectWorkspaceKind({
    packageJson: {
      private: true,
      workspaces: ['packages/*'],
    },
    hasFile: async () => false,
  });
  assert.equal(fromPackageJson.kind, 'monorepo');
  assert.deepEqual(fromPackageJson.indicators, ['package.json#workspaces']);

  const markerPath = path.join(root, 'pnpm-workspace.yaml');
  await writeFile(markerPath, 'packages:\n  - packages/*\n', 'utf8');
  const fromMarker = await detectWorkspaceKind({
    packageJson: null,
    hasFile: async (filePath) => filePath === 'pnpm-workspace.yaml',
  });
  assert.equal(fromMarker.kind, 'monorepo');
  assert.deepEqual(fromMarker.indicators, ['pnpm-workspace.yaml']);

  const single = await detectWorkspaceKind({
    packageJson: { name: 'demo-app' },
    hasFile: async () => false,
  });
  assert.equal(single.kind, 'single');
  assert.deepEqual(single.indicators, []);
});

test('formatGuardFailureMessage renders specific diagnostics', () => {
  assert.equal(
    formatGuardFailureMessage({
      name: 'node-ready',
      type: 'node_version',
      range: '>=18',
      version: '16.0.0',
    }),
    'node-ready: node version check failed (expected >=18, actual 16.0.0)',
  );
});
