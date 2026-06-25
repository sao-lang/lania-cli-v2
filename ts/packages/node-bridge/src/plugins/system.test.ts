/**
 * system 插件回归测试。
 */
import assert from 'node:assert/strict';
import { chmod, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { parseShellDiscoveryOutput, systemPlugin } from './system.js';

test('system.listCommands enumerates PATH executables and dedupes by command name', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-system-'));
  const binA = path.join(root, 'bin-a');
  const binB = path.join(root, 'bin-b');
  t.after(async () => rm(root, { recursive: true, force: true }));

  await mkdir(binA, { recursive: true });
  await mkdir(binB, { recursive: true });
  await writeExecutable(binA, 'node');
  await writeExecutable(binA, 'npm');
  await writeExecutable(binB, 'tsc');
  await writeExecutable(binB, 'node');
  await writeFile(path.join(binB, 'README.md'), '# not executable\n');

  const previousPath = process.env.PATH;
  process.env.PATH = `${binA}:${binB}`;

  try {
    const handled = await systemPlugin.handle(
      'system.listCommands',
      { cwd: root, includeShell: false },
      { cwd: root },
    );
    assert.ok(handled);
    const result = handled.result as {
      accepted: boolean;
      kind: string;
      summary: { unique: number; duplicates: number; shellBuiltins: number };
      commands: Array<{ name: string; source: string }>;
      duplicates: Array<{ name: string; paths: string[] }>;
    };
    assert.equal(result.accepted, true);
    assert.equal(result.kind, 'system_commands');
    assert.equal(result.summary.unique, 3);
    assert.equal(result.summary.duplicates, 1);
    assert.equal(result.summary.shellBuiltins, 0);
    assert.deepEqual(
      result.commands.map((command) => command.name),
      ['node', 'npm', 'tsc'],
    );
    assert.deepEqual(result.duplicates, [
      {
        name: 'node',
        paths: [path.join(binA, 'node'), path.join(binB, 'node')],
      },
    ]);
  } finally {
    process.env.PATH = previousPath;
  }
});

test('system.listCommands supports filter, limit, and allMatches', async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), 'lania-node-bridge-system-filter-'));
  const binA = path.join(root, 'bin-a');
  const binB = path.join(root, 'bin-b');
  t.after(async () => rm(root, { recursive: true, force: true }));

  await mkdir(binA, { recursive: true });
  await mkdir(binB, { recursive: true });
  await writeExecutable(binA, 'tsc');
  await writeExecutable(binA, 'tsx');
  await writeExecutable(binB, 'tsserver');
  await writeExecutable(binB, 'tsc');

  const handled = await systemPlugin.handle(
    'system.listCommands',
    {
      cwd: root,
      path: `${binA}:${binB}`,
      filter: 'ts',
      limit: 2,
      allMatches: true,
      includeShell: false,
    },
    { cwd: root },
  );

  assert.ok(handled);
  const result = handled.result as {
    summary: { returned: number };
    commands: Array<{ name: string; path: string }>;
  };
  assert.deepEqual(
    result.commands.map((command) => [command.name, command.path]),
    [
      ['tsc', path.join(binA, 'tsc')],
      ['tsc', path.join(binB, 'tsc')],
    ],
  );
  assert.equal(result.summary.returned, 2);
});

test('parseShellDiscoveryOutput parses builtin alias and function sections', () => {
  const parsed = parseShellDiscoveryOutput(`
__LANIA_BUILTINS__
cd
echo
__LANIA_ALIASES__
alias gst='git status'
alias ll='ls -al'
__LANIA_FUNCTIONS__
mkcd
take
`);

  assert.deepEqual(parsed.builtins, ['cd', 'echo']);
  assert.deepEqual(parsed.aliases, [
    { name: 'gst', expansion: 'git status' },
    { name: 'll', expansion: 'ls -al' },
  ]);
  assert.deepEqual(parsed.functions, ['mkcd', 'take']);
});

async function writeExecutable(directory: string, name: string) {
  const filePath = path.join(directory, name);
  await writeFile(filePath, '#!/usr/bin/env node\n');
  await chmod(filePath, 0o755);
}
