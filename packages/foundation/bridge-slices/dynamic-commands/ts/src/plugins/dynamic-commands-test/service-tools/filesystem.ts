import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

import {
  assert,
  createDynamicCommandProject,
  createHostRpcResponder,
  existsOnDisk,
  installHostRpcTransport,
  invokeManifestHandler,
  readFile,
  readdir,
  resetHostRpcTransport,
  rm,
  respondUnsupportedTestHostMethod,
  stat,
  type TestHostRpcPayload,
} from '../shared.js';

export function registerFilesystemServiceTests() {
  test('command.invokeDynamic supports fs helpers and high-level operations', async (t) => {
    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        const cwd = String(payload.params.cwd ?? '');
        const targetPath = String(payload.params.path ?? '');
        const resolved = path.isAbsolute(targetPath) ? targetPath : path.resolve(cwd, targetPath);
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.fs.exists':
            respond({ exists: await existsOnDisk(resolved) });
            return;
          case 'host.fs.read':
            respond({ content: await readFile(resolved, 'utf8') });
            return;
          case 'host.fs.write':
            await mkdir(path.dirname(resolved), { recursive: true });
            await writeFile(resolved, String(payload.params.content ?? ''), {
              flag: payload.params.append ? 'a' : 'w',
            });
            respond({ ok: true });
            return;
          case 'host.fs.mkdirp':
            await mkdir(resolved, { recursive: true });
            respond({ ok: true });
            return;
          case 'host.fs.remove':
            await rm(resolved, {
              recursive: Boolean(payload.params.recursive),
              force: true,
            });
            respond({ removed: true });
            return;
          case 'host.fs.readdir':
            respond({ entries: await readdir(resolved) });
            return;
          case 'host.fs.stat': {
            const stats = await stat(resolved);
            respond({
              isFile: stats.isFile(),
              isDir: stats.isDirectory(),
              size: stats.size,
              mtimeMs: stats.mtimeMs,
            });
            return;
          }
          default:
            respondUnsupportedTestHostMethod(payload);
        }
      },
    });
    t.after(() => resetHostRpcTransport());

    const cwd = await createDynamicCommandProject({
      manifestContent: `export default {
        runtimeCommands: [
          {
            mount: 'ops',
            commands: [
              {
                name: 'fs-helpers',
                handler: async (ctx) => {
                  await ctx.tools.fs.ensureDir('tmp/a');
                  await ctx.tools.fs.ensureFile('tmp/a/file.txt', 'hello');
                  await ctx.tools.fs.append('tmp/a/file.txt', ' world');
                  await ctx.tools.fs.replace('tmp/a/file.txt', 'world', 'lania');
                  await ctx.tools.fs.mkdir('tmp/b');
                  await ctx.tools.fs.copy('tmp/a/file.txt', 'tmp/b/copied.txt');
                  await ctx.tools.fs.move('tmp/b/copied.txt', 'tmp/moved.txt');
                  await ctx.tools.fs.write('tmp/replace-regex.txt', 'a+b+c');
                  await ctx.tools.fs.replace('tmp/replace-regex.txt', /\\+/g, '-');
                  const globbed = await ctx.tools.fs.glob('tmp/**/*.txt');
                  const movedContent = await ctx.tools.fs.read('tmp/moved.txt');
                  const replacedContent = await ctx.tools.fs.read('tmp/replace-regex.txt');
                  const entries = await ctx.tools.fs.readdir('tmp');
                  const movedStat = await ctx.tools.fs.stat('tmp/moved.txt');
                  const existsBeforeRemove = await ctx.tools.fs.exists('tmp/moved.txt');
                  await ctx.tools.fs.remove('tmp/moved.txt');
                  const existsAfterRemove = await ctx.tools.fs.exists('tmp/moved.txt');
                  return ctx.tools.result.ok({
                    globbed,
                    movedContent,
                    replacedContent,
                    entries,
                    movedStat,
                    existsBeforeRemove,
                    existsAfterRemove
                  });
                }
              }
            ]
          }
        ]
      };`,
    });
    t.after(async () => rm(cwd, { recursive: true, force: true }));

    const invoked = await invokeManifestHandler({
      cwd,
      commandName: 'fs-helpers',
      requestIdPrefix: 'req-fs-helpers',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.deepEqual(payload?.globbed, [
      'tmp/a/file.txt',
      'tmp/moved.txt',
      'tmp/replace-regex.txt',
    ]);
    assert.equal(payload?.movedContent, 'hello lania');
    assert.equal(payload?.replacedContent, 'a-b-c');
    assert.deepEqual(payload?.entries, ['a', 'b', 'moved.txt', 'replace-regex.txt']);
    assert.equal(payload?.movedStat?.isFile, true);
    assert.equal(payload?.existsBeforeRemove, true);
    assert.equal(payload?.existsAfterRemove, false);
  });
}
