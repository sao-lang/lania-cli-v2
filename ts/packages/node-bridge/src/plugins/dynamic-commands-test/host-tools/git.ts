import test from 'node:test';

import {
  assert,
  createDynamicCommandProject,
  createHostRpcResponder,
  installHostRpcTransport,
  invokeManifestHandler,
  resetHostRpcTransport,
  rm,
  respondUnsupportedTestHostMethod,
  type TestHostRpcPayload,
} from '../shared.js';

export function registerGroupedGitHelperTests() {
  test('command.invokeDynamic supports grouped git helpers', async (t) => {
    const gitCalls: Array<{ method: string; params: Record<string, unknown> }> = [];

    installHostRpcTransport({
      write: async (envelope) => {
        const payload = envelope.payload as TestHostRpcPayload;
        gitCalls.push({ method: payload.method, params: payload.params });
        const respond = createHostRpcResponder(payload);

        switch (payload.method) {
          case 'host.git.git.status':
          case 'host.git.workspace.status':
            respond({ ready: true, branch: 'feature/demo' });
            return;
          case 'host.git.git.isInstalled':
            respond({ installed: true });
            return;
          case 'host.git.git.version':
            respond({ version: 'git version 2.42.0' });
            return;
          case 'host.git.git.isInit':
            respond({ isInit: true });
            return;
          case 'host.git.git.init':
          case 'host.git.git.clone':
          case 'host.git.branch.create':
          case 'host.git.branch.switch':
          case 'host.git.branch.delete':
          case 'host.git.branch.merge':
          case 'host.git.branch.mergeWithOptions':
          case 'host.git.branch.mergeNoFF':
          case 'host.git.branch.abortMerge':
          case 'host.git.branch.cherryPick':
          case 'host.git.branch.continueCherryPick':
          case 'host.git.branch.abortCherryPick':
          case 'host.git.branch.rebase':
          case 'host.git.branch.abortRebase':
          case 'host.git.branch.continueRebase':
          case 'host.git.branch.skipRebase':
          case 'host.git.branch.setUpstream':
          case 'host.git.remote.add':
          case 'host.git.remote.pull':
          case 'host.git.remote.push':
          case 'host.git.stage.add':
          case 'host.git.stage.addAll':
          case 'host.git.stage.reset':
          case 'host.git.workspace.commit':
          case 'host.git.workspace.commitAmend':
          case 'host.git.workspace.revert':
          case 'host.git.workspace.abortRevert':
          case 'host.git.workspace.continueRevert':
          case 'host.git.user.set':
          case 'host.git.tag.create':
          case 'host.git.tag.delete':
            respond({ ok: true });
            return;
          case 'host.git.listBranches':
            respond({ local: ['main', 'feature/demo'], remote: ['origin/main'] });
            return;
          case 'host.git.branch.listLocal':
            respond({ branches: ['main', 'feature/demo'] });
            return;
          case 'host.git.branch.listRemote':
            respond({ branches: ['origin/main'] });
            return;
          case 'host.git.branch.listAll':
            respond({ local: ['main', 'feature/demo'], remote: ['origin/main'] });
            return;
          case 'host.git.branch.current':
            respond({ branch: 'feature/demo' });
            return;
          case 'host.git.branch.exists':
          case 'host.git.branch.existsLocal':
            respond({ exists: true });
            return;
          case 'host.git.branch.existsRemote':
            respond({ exists: false });
            return;
          case 'host.git.branch.upstream':
            respond({ remote: 'origin', branch: 'main' });
            return;
          case 'host.git.branch.needsUpstream':
            respond({ needsUpstream: false });
            return;
          case 'host.git.branch.hasUnpushedCommits':
            respond({ hasUnpushedCommits: true });
            return;
          case 'host.git.remote.list':
            respond([{ name: 'origin', url: 'https://example.com/repo.git' }]);
            return;
          case 'host.git.remote.exists':
            respond({ exists: true });
            return;
          case 'host.git.remote.status':
            respond({ status: 'up-to-date' });
            return;
          case 'host.git.stage.files':
            respond({ files: ['src/index.ts'] });
            return;
          case 'host.git.stage.diff':
            respond({ diff: 'diff --git a/src/index.ts b/src/index.ts' });
            return;
          case 'host.git.workspace.statusPorcelain':
            respond({ lines: ['M src/index.ts', '?? notes.md'] });
            return;
          case 'host.git.workspace.changedFiles':
            respond({ files: ['src/index.ts', 'notes.md'] });
            return;
          case 'host.git.workspace.isClean':
            respond({ isClean: false });
            return;
          case 'host.git.workspace.hasChanges':
            respond({ hasChanges: true });
            return;
          case 'host.git.workspace.lastCommitMessage':
            respond({ message: 'feat: demo' });
            return;
          case 'host.git.workspace.lastCommitHash':
            respond({ hash: 'abc123' });
            return;
          case 'host.git.workspace.commitFiles':
            respond({ files: ['src/index.ts', 'README.md'] });
            return;
          case 'host.git.workspace.commitLog':
            respond([{ hash: 'abc123', message: 'feat: demo' }]);
            return;
          case 'host.git.user.get':
            respond({ name: 'Demo', email: 'demo@example.com' });
            return;
          case 'host.git.tag.list':
            respond({ tags: ['v1.0.0'] });
            return;
          case 'host.git.plan.addAll':
            respond({ program: 'git', args: ['add', '-A'] });
            return;
          case 'host.git.plan.init':
            respond({ program: 'git', args: ['init'] });
            return;
          case 'host.git.plan.commitMessage':
            respond({
              program: 'git',
              args: ['commit', '-m', String(payload.params.message ?? '')],
            });
            return;
          case 'host.git.plan.commitAmend':
            respond({
              program: 'git',
              args:
                typeof payload.params.message === 'string'
                  ? ['commit', '--amend', '-m', String(payload.params.message)]
                  : ['commit', '--amend', '--no-edit'],
            });
            return;
          case 'host.git.plan.push':
            respond({
              program: 'git',
              args: ['push', String(payload.params.remote), String(payload.params.branch)],
            });
            return;
          case 'host.git.plan.pushTag':
            respond({
              program: 'git',
              args: ['push', String(payload.params.remote), String(payload.params.tag)],
            });
            return;
          case 'host.git.plan.tagCreateLightweight':
            respond({ program: 'git', args: ['tag', String(payload.params.tag)] });
            return;
          case 'host.git.plan.tagCreateAnnotated':
            respond({
              program: 'git',
              args: ['tag', String(payload.params.tag), '-m', String(payload.params.message)],
            });
            return;
          case 'host.git.plan.tagDelete':
            respond({ program: 'git', args: ['tag', '-d', String(payload.params.tag)] });
            return;
          case 'host.git.command':
            respond({ program: 'git', args: (payload.params.args as string[] | undefined) ?? [] });
            return;
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
                name: 'git-groups',
                handler: async (ctx) => {
                  const status = await ctx.tools.git.status();
                  const changedFiles = await ctx.tools.git.changedFiles();
                  const gitStatus = await ctx.tools.git.git.status();
                  const installed = await ctx.tools.git.git.isInstalled();
                  const version = await ctx.tools.git.git.version();
                  const isInit = await ctx.tools.git.git.isInit();
                  await ctx.tools.git.git.init();
                  await ctx.tools.git.git.clone('https://example.com/repo.git', 'tmp/repo');
                  const branchList = await ctx.tools.git.branch.list();
                  const branchLocal = await ctx.tools.git.branch.listLocal();
                  const branchRemote = await ctx.tools.git.branch.listRemote();
                  const branchAll = await ctx.tools.git.branch.listAll();
                  const current = await ctx.tools.git.branch.current();
                  const exists = await ctx.tools.git.branch.exists('main');
                  const existsLocal = await ctx.tools.git.branch.existsLocal('main');
                  const existsRemote = await ctx.tools.git.branch.existsRemote('origin/main');
                  await ctx.tools.git.branch.create('release/demo');
                  await ctx.tools.git.branch.switch('release/demo');
                  await ctx.tools.git.branch.delete('release/demo', { force: true });
                  await ctx.tools.git.branch.merge('main');
                  await ctx.tools.git.branch.mergeWithOptions('develop', {
                    strategy: 'ours',
                    message: 'merge develop',
                    flags: ['--log']
                  });
                  await ctx.tools.git.branch.mergeNoFF('release/demo');
                  await ctx.tools.git.branch.abortMerge();
                  await ctx.tools.git.branch.cherryPick('abc123');
                  await ctx.tools.git.branch.continueCherryPick();
                  await ctx.tools.git.branch.abortCherryPick();
                  await ctx.tools.git.branch.rebase('main', {
                    interactive: true,
                    onto: 'origin/main',
                    root: true
                  });
                  await ctx.tools.git.branch.abortRebase();
                  await ctx.tools.git.branch.continueRebase();
                  await ctx.tools.git.branch.skipRebase();
                  const upstream = await ctx.tools.git.branch.upstream();
                  const needsUpstream = await ctx.tools.git.branch.needsUpstream();
                  await ctx.tools.git.branch.setUpstream('origin', 'main');
                  const hasUnpushedCommits = await ctx.tools.git.branch.hasUnpushedCommits();
                  const remotes = await ctx.tools.git.remotes();
                  const remoteExists = await ctx.tools.git.remoteExists('origin');
                  const remoteList = await ctx.tools.git.remote.list();
                  await ctx.tools.git.remote.add('upstream', 'https://example.com/upstream.git');
                  await ctx.tools.git.remote.pull('origin', 'main');
                  await ctx.tools.git.remote.push('origin', 'main');
                  const remoteStatus = await ctx.tools.git.remote.status('origin');
                  const stagedFiles = await ctx.tools.git.stage.files();
                  await ctx.tools.git.stage.add(['src/index.ts']);
                  await ctx.tools.git.stage.addAll();
                  await ctx.tools.git.stage.reset('src/index.ts');
                  const stageDiff = await ctx.tools.git.stage.diff();
                  const workspaceStatus = await ctx.tools.git.workspace.status();
                  const porcelain = await ctx.tools.git.workspace.statusPorcelain();
                  const workspaceChanged = await ctx.tools.git.workspace.changedFiles();
                  const clean = await ctx.tools.git.workspace.isClean();
                  const hasChanges = await ctx.tools.git.workspace.hasChanges();
                  await ctx.tools.git.workspace.commit('feat: demo');
                  await ctx.tools.git.workspace.commitAmend({ noEdit: true });
                  const lastMessage = await ctx.tools.git.workspace.lastCommitMessage();
                  const lastHash = await ctx.tools.git.workspace.lastCommitHash();
                  const commitFiles = await ctx.tools.git.workspace.commitFiles('abc123');
                  const commitLog = await ctx.tools.git.commitLog({ limit: 1 });
                  await ctx.tools.git.workspace.revert(['abc123'], {
                    noCommit: true,
                    mainline: 1,
                    noEdit: true
                  });
                  await ctx.tools.git.workspace.abortRevert();
                  await ctx.tools.git.workspace.continueRevert();
                  const user = await ctx.tools.git.user.get();
                  await ctx.tools.git.user.set('Demo', 'demo@example.com');
                  const tags = await ctx.tools.git.tag.list();
                  await ctx.tools.git.tag.create('v1.0.0', { annotated: true, message: 'release' });
                  await ctx.tools.git.tag.delete('v1.0.0');
                  const planAddAll = await ctx.tools.git.plan.addAll();
                  const planPush = await ctx.tools.git.plan.push('origin', 'main');
                  const planAnnotated = await ctx.tools.git.plan.tagCreateAnnotated('v1.0.0', 'release');
                  const command = await ctx.tools.git.command(['status', '--short']);
                  return ctx.tools.result.ok({
                    status,
                    changedFiles,
                    gitStatus,
                    installed,
                    version,
                    isInit,
                    branchList,
                    branchLocal,
                    branchRemote,
                    branchAll,
                    current,
                    exists,
                    existsLocal,
                    existsRemote,
                    upstream,
                    needsUpstream,
                    hasUnpushedCommits,
                    remotes,
                    remoteExists,
                    remoteList,
                    remoteStatus,
                    stagedFiles,
                    stageDiff,
                    workspaceStatus,
                    porcelain,
                    workspaceChanged,
                    clean,
                    hasChanges,
                    lastMessage,
                    lastHash,
                    commitFiles,
                    commitLog,
                    user,
                    tags,
                    planAddAll,
                    planPush,
                    planAnnotated,
                    command
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
      commandName: 'git-groups',
      requestIdPrefix: 'req-git-groups',
    });

    const payload = (invoked.response.result as any)?.result?.data;
    assert.equal(payload?.status?.branch, 'feature/demo');
    assert.deepEqual(payload?.changedFiles, ['src/index.ts', 'notes.md']);
    assert.equal(payload?.gitStatus?.ready, true);
    assert.equal(payload?.installed, true);
    assert.equal(payload?.version, 'git version 2.42.0');
    assert.equal(payload?.isInit, true);
    assert.deepEqual(payload?.branchList, {
      local: ['main', 'feature/demo'],
      remote: ['origin/main'],
    });
    assert.deepEqual(payload?.branchLocal, ['main', 'feature/demo']);
    assert.deepEqual(payload?.branchRemote, ['origin/main']);
    assert.deepEqual(payload?.branchAll, {
      local: ['main', 'feature/demo'],
      remote: ['origin/main'],
    });
    assert.equal(payload?.current, 'feature/demo');
    assert.equal(payload?.exists, true);
    assert.equal(payload?.existsLocal, true);
    assert.equal(payload?.existsRemote, false);
    assert.deepEqual(payload?.upstream, { remote: 'origin', branch: 'main' });
    assert.equal(payload?.needsUpstream, false);
    assert.equal(payload?.hasUnpushedCommits, true);
    assert.deepEqual(payload?.remotes, [{ name: 'origin', url: 'https://example.com/repo.git' }]);
    assert.equal(payload?.remoteExists, true);
    assert.equal(payload?.remoteStatus, 'up-to-date');
    assert.deepEqual(payload?.stagedFiles, ['src/index.ts']);
    assert.equal(payload?.stageDiff, 'diff --git a/src/index.ts b/src/index.ts');
    assert.deepEqual(payload?.porcelain, ['M src/index.ts', '?? notes.md']);
    assert.equal(payload?.clean, false);
    assert.equal(payload?.hasChanges, true);
    assert.equal(payload?.lastMessage, 'feat: demo');
    assert.equal(payload?.lastHash, 'abc123');
    assert.deepEqual(payload?.commitFiles, ['src/index.ts', 'README.md']);
    assert.deepEqual(payload?.commitLog, [{ hash: 'abc123', message: 'feat: demo' }]);
    assert.deepEqual(payload?.user, { name: 'Demo', email: 'demo@example.com' });
    assert.deepEqual(payload?.tags, ['v1.0.0']);
    assert.deepEqual(payload?.planAddAll, {
      program: 'git',
      args: ['add', '-A'],
      env: {},
      useShell: false,
    });
    assert.deepEqual(payload?.planPush, {
      program: 'git',
      args: ['push', 'origin', 'main'],
      env: {},
      useShell: false,
    });
    assert.deepEqual(payload?.planAnnotated, {
      program: 'git',
      args: ['tag', 'v1.0.0', '-m', 'release'],
      env: {},
      useShell: false,
    });
    assert.deepEqual(payload?.command, {
      program: 'git',
      args: ['status', '--short'],
      env: {},
      useShell: false,
    });

    const cloneCall = gitCalls.find((entry) => entry.method === 'host.git.git.clone');
    const mergeWithOptionsCall = gitCalls.find(
      (entry) => entry.method === 'host.git.branch.mergeWithOptions',
    );
    const rebaseCall = gitCalls.find((entry) => entry.method === 'host.git.branch.rebase');
    const revertCall = gitCalls.find((entry) => entry.method === 'host.git.workspace.revert');
    assert.equal(cloneCall?.params?.repoUrl, 'https://example.com/repo.git');
    assert.equal(cloneCall?.params?.targetDir, 'tmp/repo');
    assert.deepEqual(mergeWithOptionsCall?.params?.flags, ['--log']);
    assert.equal(mergeWithOptionsCall?.params?.strategy, 'ours');
    assert.equal(rebaseCall?.params?.interactive, true);
    assert.equal(rebaseCall?.params?.onto, 'origin/main');
    assert.equal(rebaseCall?.params?.root, true);
    assert.deepEqual(revertCall?.params?.commits, ['abc123']);
    assert.equal(revertCall?.params?.noCommit, true);
    assert.equal(revertCall?.params?.mainline, 1);
    assert.equal(revertCall?.params?.noEdit, true);
  });
}
