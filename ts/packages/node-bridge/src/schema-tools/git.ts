/**
 * `tools.git` 的 host-backed facade。
 *
 * 这一层不直接实现 git 行为，而是把 schema 侧常用的 git 能力整理成稳定 API：
 * - 顶层保留 v2.3 兼容的 legacy shortcut，如 `status`、`commit`；
 * - 分组命名空间提供更清晰的 `branch` / `remote` / `workspace` / `tag` / `plan` 视图；
 * - 所有真正执行或查询仓库状态的动作最终都桥接回 host。
 *
 * 这里的重点不是“封装 git 命令字符串”，而是统一：
 * - 默认 `cwd` 继承规则
 * - git 策略校验入口
 * - host-rpc 返回值的结构归一化
 */
import { hostCall } from '../core/host-rpc.js';
import { asRecord } from '../core/runtime.js';
import type { ToolsPolicyManager } from './policy.js';
import type { SchemaToolContext } from './types.js';

// 这些 DTO 有意保持精简，只暴露 schema 作者真正会消费的字段，
// 而不是把 host 侧内部状态原样泄漏出来。
export type GitStatusResult = { ready: boolean; branch?: string | null };
export type GitRemoteInfo = { name: string; url: string };
export type GitUserInfo = { name: string; email: string };
export type GitUpstreamInfo = { remote: string; branch: string };
export type GitCommitLogEntry = { hash: string; message: string };
export type GitExecCommand = {
  program: string;
  args: string[];
  cwd?: string;
  env?: Record<string, string>;
  useShell?: boolean;
};

export type GitCommitLogOptionsInput = {
  cwd?: string;
  limit?: number;
  author?: string;
  since?: string;
  until?: string;
  oneline?: boolean;
  format?: string;
};

// 公开接口同时保留：
// - 历史顶层快捷方法，避免已有 schema 失效
// - 更易维护的分组 API，便于后续继续扩展 git 子域能力
export interface GitTools {
  status: (cwd?: string) => Promise<GitStatusResult>;
  isClean: (cwd?: string) => Promise<boolean>;
  changedFiles: (cwd?: string) => Promise<string[]>;
  remotes: (cwd?: string) => Promise<GitRemoteInfo[]>;
  remoteExists: (remote: string, cwd?: string) => Promise<boolean>;
  commit: (message: string, cwd?: string) => Promise<void>;
  lastCommitMessage: (cwd?: string) => Promise<string>;
  lastCommitHash: (cwd?: string) => Promise<string>;
  commitLog: (options?: GitCommitLogOptionsInput) => Promise<GitCommitLogEntry[]>;
  command: (args: string[]) => Promise<GitExecCommand>;
  git: {
    init: (cwd?: string) => Promise<void>;
    isInstalled: () => Promise<boolean>;
    version: () => Promise<string>;
    isInit: (cwd?: string) => Promise<boolean>;
    clone: (repoUrl: string, targetDir?: string, cwd?: string) => Promise<void>;
    status: (cwd?: string) => Promise<GitStatusResult>;
  };
  branch: {
    current: (cwd?: string) => Promise<string | null>;
    list: (options?: { cwd?: string; scope?: 'local' | 'remote' | 'all' }) => Promise<{
      local?: string[];
      remote?: string[];
    }>;
    listLocal: (cwd?: string) => Promise<string[]>;
    listRemote: (cwd?: string) => Promise<string[]>;
    listAll: (cwd?: string) => Promise<{ local: string[]; remote: string[] }>;
    exists: (branch: string, cwd?: string) => Promise<boolean>;
    existsLocal: (branch: string, cwd?: string) => Promise<boolean>;
    existsRemote: (branch: string, cwd?: string) => Promise<boolean>;
    create: (branch: string, cwd?: string) => Promise<void>;
    switch: (branch: string, cwd?: string) => Promise<void>;
    delete: (branch: string, options?: { cwd?: string; force?: boolean }) => Promise<void>;
    merge: (branch: string, cwd?: string) => Promise<void>;
    mergeWithOptions: (
      branch: string,
      options?: { cwd?: string; flags?: string[]; strategy?: string; message?: string },
    ) => Promise<void>;
    mergeNoFF: (branch: string, cwd?: string) => Promise<void>;
    abortMerge: (cwd?: string) => Promise<void>;
    cherryPick: (commit: string, cwd?: string) => Promise<void>;
    continueCherryPick: (cwd?: string) => Promise<void>;
    abortCherryPick: (cwd?: string) => Promise<void>;
    rebase: (
      targetBranch: string,
      options?: { cwd?: string; interactive?: boolean; onto?: string; root?: boolean },
    ) => Promise<void>;
    abortRebase: (cwd?: string) => Promise<void>;
    continueRebase: (cwd?: string) => Promise<void>;
    skipRebase: (cwd?: string) => Promise<void>;
    upstream: (cwd?: string) => Promise<GitUpstreamInfo | null>;
    needsUpstream: (cwd?: string) => Promise<boolean>;
    setUpstream: (remote: string, branch: string, cwd?: string) => Promise<void>;
    hasUnpushedCommits: (cwd?: string) => Promise<boolean>;
  };
  remote: {
    list: (cwd?: string) => Promise<GitRemoteInfo[]>;
    exists: (remote: string, cwd?: string) => Promise<boolean>;
    add: (name: string, url: string, cwd?: string) => Promise<void>;
    pull: (remote: string, branch: string, cwd?: string) => Promise<void>;
    push: (remote: string, branch: string, cwd?: string) => Promise<void>;
    status: (remote: string, cwd?: string) => Promise<string>;
  };
  stage: {
    files: (cwd?: string) => Promise<string[]>;
    add: (files: string[], cwd?: string) => Promise<void>;
    addAll: (cwd?: string) => Promise<void>;
    reset: (file: string, cwd?: string) => Promise<void>;
    diff: (cwd?: string) => Promise<string>;
  };
  workspace: {
    status: (cwd?: string) => Promise<GitStatusResult>;
    statusPorcelain: (cwd?: string) => Promise<string[]>;
    changedFiles: (cwd?: string) => Promise<string[]>;
    isClean: (cwd?: string) => Promise<boolean>;
    hasChanges: (cwd?: string) => Promise<boolean>;
    commit: (message: string, cwd?: string) => Promise<void>;
    commitAmend: (options?: { cwd?: string; message?: string; noEdit?: boolean }) => Promise<void>;
    lastCommitMessage: (cwd?: string) => Promise<string>;
    lastCommitHash: (cwd?: string) => Promise<string>;
    commitFiles: (commit: string, cwd?: string) => Promise<string[]>;
    commitLog: (options?: GitCommitLogOptionsInput) => Promise<GitCommitLogEntry[]>;
    revert: (
      commits: string[],
      options?: { cwd?: string; noCommit?: boolean; mainline?: number; noEdit?: boolean },
    ) => Promise<void>;
    abortRevert: (cwd?: string) => Promise<void>;
    continueRevert: (cwd?: string) => Promise<void>;
  };
  user: {
    get: (cwd?: string) => Promise<GitUserInfo>;
    set: (name: string, email: string, cwd?: string) => Promise<void>;
  };
  tag: {
    list: (cwd?: string) => Promise<string[]>;
    create: (
      tag: string,
      options?: { cwd?: string; annotated?: boolean; message?: string },
    ) => Promise<void>;
    delete: (tag: string, cwd?: string) => Promise<void>;
  };
  plan: {
    addAll: () => Promise<GitExecCommand>;
    init: () => Promise<GitExecCommand>;
    commitMessage: (message: string) => Promise<GitExecCommand>;
    commitAmend: (message?: string, noEdit?: boolean) => Promise<GitExecCommand>;
    push: (remote: string, branch: string) => Promise<GitExecCommand>;
    pushTag: (remote: string, tag: string) => Promise<GitExecCommand>;
    tagCreateLightweight: (tag: string) => Promise<GitExecCommand>;
    tagCreateAnnotated: (tag: string, message: string) => Promise<GitExecCommand>;
    tagDelete: (tag: string) => Promise<GitExecCommand>;
  };
}

export function createGitTools(base: SchemaToolContext, policy: ToolsPolicyManager): GitTools {
  // 所有 host-backed git 调用默认继承 `base.cwd`，除非调用方显式覆写。
  // 这样整个命名空间的参数契约保持一致。
  const withCwd = (cwd?: string, params?: Record<string, unknown>) => ({
    cwd: cwd ?? base.cwd,
    ...(params ?? {}),
  });

  // 整个 git 命名空间只在这里做一次策略校验；
  // 下面各个子域对象只描述业务语义，不重复关心 allow/deny 逻辑。
  const gitCall = async <T>(
    method: string,
    operation: string,
    params?: Record<string, unknown>,
  ): Promise<T> => {
    await policy.assertGitAllowed(operation);
    // host-rpc 细节统一收口到这里，子域 helper 只负责描述 git 语义。
    const exchange = await hostCall<T>(method, params);
    return exchange.result;
  };

  // `plan.*` 返回的是“可执行命令快照”，而不是立即运行 git。
  // 这里会把 host 侧结果归一成 exec 兼容 DTO，方便 schema 侧决定是展示、审计还是稍后执行。
  const gitCommand = async (
    method: string,
    operation: string,
    params?: Record<string, unknown>,
  ): Promise<GitExecCommand> => {
    const result = await gitCall<GitExecCommand>(method, operation, params);
    const envRecord = Object.fromEntries(
      Object.entries(asRecord(result?.env)).filter(
        (entry): entry is [string, string] => typeof entry[1] === 'string',
      ),
    );
    return {
      program: String(result?.program ?? 'git'),
      args: Array.isArray(result?.args) ? result.args.map((item) => String(item)) : [],
      // `cwd` 缺失时直接省略；`env/useShell` 则统一归一，保证调用方处理结果时结构稳定。
      ...(typeof result?.cwd === 'string' ? { cwd: result.cwd } : {}),
      env: envRecord,
      useShell: Boolean(result?.useShell),
    };
  };

  // `workspace` 负责“仓库当前状态”和“提交历史”这两类工作区语义。
  // 稍后的顶层别名会转发到这里，以保持历史 API 兼容。
  const workspace = {
    status: async (cwd?: string) =>
      gitCall<GitStatusResult>('host.git.workspace.status', 'workspace.status', withCwd(cwd)),
    statusPorcelain: async (cwd?: string) => {
      const result = await gitCall<{ lines: string[] }>(
        'host.git.workspace.statusPorcelain',
        'workspace.statusPorcelain',
        withCwd(cwd),
      );
      return Array.isArray(result.lines) ? result.lines : [];
    },
    changedFiles: async (cwd?: string) => {
      const result = await gitCall<{ files: string[] }>(
        'host.git.workspace.changedFiles',
        'workspace.changedFiles',
        withCwd(cwd),
      );
      return Array.isArray(result.files) ? result.files : [];
    },
    isClean: async (cwd?: string) => {
      const result = await gitCall<{ isClean: boolean }>(
        'host.git.workspace.isClean',
        'workspace.isClean',
        withCwd(cwd),
      );
      return Boolean(result.isClean);
    },
    hasChanges: async (cwd?: string) => {
      const result = await gitCall<{ hasChanges: boolean }>(
        'host.git.workspace.hasChanges',
        'workspace.hasChanges',
        withCwd(cwd),
      );
      return Boolean(result.hasChanges);
    },
    commit: async (message: string, cwd?: string) => {
      await gitCall('host.git.workspace.commit', 'workspace.commit', withCwd(cwd, { message }));
    },
    commitAmend: async (options?: { cwd?: string; message?: string; noEdit?: boolean }) => {
      await gitCall(
        'host.git.workspace.commitAmend',
        'workspace.commitAmend',
        withCwd(options?.cwd, { message: options?.message, noEdit: options?.noEdit ?? false }),
      );
    },
    lastCommitMessage: async (cwd?: string) => {
      const result = await gitCall<{ message: string }>(
        'host.git.workspace.lastCommitMessage',
        'workspace.lastCommitMessage',
        withCwd(cwd),
      );
      return String(result.message ?? '');
    },
    lastCommitHash: async (cwd?: string) => {
      const result = await gitCall<{ hash: string }>(
        'host.git.workspace.lastCommitHash',
        'workspace.lastCommitHash',
        withCwd(cwd),
      );
      return String(result.hash ?? '');
    },
    commitFiles: async (commit: string, cwd?: string) => {
      const result = await gitCall<{ files: string[] }>(
        'host.git.workspace.commitFiles',
        'workspace.commitFiles',
        withCwd(cwd, { commit }),
      );
      return Array.isArray(result.files) ? result.files : [];
    },
    commitLog: async (options?: GitCommitLogOptionsInput) => {
      // `workspace.commitLog` is the canonical history reader. Top-level
      // `commitLog()` later aliases this helper for backward compatibility.
      const result = await gitCall<GitCommitLogEntry[]>(
        'host.git.workspace.commitLog',
        'workspace.commitLog',
        withCwd(options?.cwd, {
          limit: options?.limit,
          author: options?.author,
          since: options?.since,
          until: options?.until,
          oneline: options?.oneline ?? false,
          format: options?.format,
        }),
      );
      return Array.isArray(result) ? result : [];
    },
    revert: async (
      commits: string[],
      options?: { cwd?: string; noCommit?: boolean; mainline?: number; noEdit?: boolean },
    ) => {
      await gitCall(
        'host.git.workspace.revert',
        'workspace.revert',
        withCwd(options?.cwd, {
          commits,
          noCommit: options?.noCommit ?? false,
          mainline: options?.mainline,
          noEdit: options?.noEdit ?? false,
        }),
      );
    },
    abortRevert: async (cwd?: string) => {
      await gitCall('host.git.workspace.abortRevert', 'workspace.abortRevert', withCwd(cwd));
    },
    continueRevert: async (cwd?: string) => {
      await gitCall('host.git.workspace.continueRevert', 'workspace.continueRevert', withCwd(cwd));
    },
  };

  // `branch` 同时覆盖状态查询和改写历史的动作。
  // 调用方始终传用户视角的分支名，transport 字段映射留在这一层处理。
  const branch = {
    current: async (cwd?: string) => {
      const result = await gitCall<{ branch: string | null }>(
        'host.git.branch.current',
        'branch.current',
        withCwd(cwd),
      );
      return result.branch ?? null;
    },
    // `list()` 有意保留 host 的 `{ local?, remote? }` 包装形状，
    // 这样调用方只查某个 scope 时，不需要额外为另一侧结果做归一化。
    list: async (options?: { cwd?: string; scope?: 'local' | 'remote' | 'all' }) =>
      gitCall<{ local?: string[]; remote?: string[] }>('host.git.listBranches', 'branch.list', {
        cwd: options?.cwd ?? base.cwd,
        scope: options?.scope ?? 'all',
      }),
    listLocal: async (cwd?: string) => {
      const result = await gitCall<{ branches: string[] }>(
        'host.git.branch.listLocal',
        'branch.listLocal',
        withCwd(cwd),
      );
      return Array.isArray(result.branches) ? result.branches : [];
    },
    listRemote: async (cwd?: string) => {
      const result = await gitCall<{ branches: string[] }>(
        'host.git.branch.listRemote',
        'branch.listRemote',
        withCwd(cwd),
      );
      return Array.isArray(result.branches) ? result.branches : [];
    },
    listAll: async (cwd?: string) => {
      const result = await gitCall<{ local: string[]; remote: string[] }>(
        'host.git.branch.listAll',
        'branch.listAll',
        withCwd(cwd),
      );
      return {
        local: Array.isArray(result.local) ? result.local : [],
        remote: Array.isArray(result.remote) ? result.remote : [],
      };
    },
    exists: async (branchName: string, cwd?: string) => {
      const result = await gitCall<{ exists: boolean }>(
        'host.git.branch.exists',
        'branch.exists',
        withCwd(cwd, { branch: branchName }),
      );
      return Boolean(result.exists);
    },
    existsLocal: async (branchName: string, cwd?: string) => {
      const result = await gitCall<{ exists: boolean }>(
        'host.git.branch.existsLocal',
        'branch.existsLocal',
        withCwd(cwd, { branch: branchName }),
      );
      return Boolean(result.exists);
    },
    existsRemote: async (branchName: string, cwd?: string) => {
      const result = await gitCall<{ exists: boolean }>(
        'host.git.branch.existsRemote',
        'branch.existsRemote',
        withCwd(cwd, { branch: branchName }),
      );
      return Boolean(result.exists);
    },
    create: async (branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.branch.create',
        'branch.create',
        withCwd(cwd, { branch: branchName }),
      );
    },
    switch: async (branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.branch.switch',
        'branch.switch',
        withCwd(cwd, { branch: branchName }),
      );
    },
    delete: async (branchName: string, options?: { cwd?: string; force?: boolean }) => {
      await gitCall(
        'host.git.branch.delete',
        'branch.delete',
        withCwd(options?.cwd, { branch: branchName, force: options?.force ?? false }),
      );
    },
    merge: async (branchName: string, cwd?: string) => {
      await gitCall('host.git.branch.merge', 'branch.merge', withCwd(cwd, { branch: branchName }));
    },
    mergeWithOptions: async (
      branchName: string,
      options?: { cwd?: string; flags?: string[]; strategy?: string; message?: string },
    ) => {
      await gitCall(
        'host.git.branch.mergeWithOptions',
        'branch.mergeWithOptions',
        withCwd(options?.cwd, {
          branch: branchName,
          flags: options?.flags ?? [],
          strategy: options?.strategy,
          message: options?.message,
        }),
      );
    },
    mergeNoFF: async (branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.branch.mergeNoFF',
        'branch.mergeNoFF',
        withCwd(cwd, { branch: branchName }),
      );
    },
    abortMerge: async (cwd?: string) => {
      await gitCall('host.git.branch.abortMerge', 'branch.abortMerge', withCwd(cwd));
    },
    cherryPick: async (commit: string, cwd?: string) => {
      await gitCall('host.git.branch.cherryPick', 'branch.cherryPick', withCwd(cwd, { commit }));
    },
    continueCherryPick: async (cwd?: string) => {
      await gitCall(
        'host.git.branch.continueCherryPick',
        'branch.continueCherryPick',
        withCwd(cwd),
      );
    },
    abortCherryPick: async (cwd?: string) => {
      await gitCall('host.git.branch.abortCherryPick', 'branch.abortCherryPick', withCwd(cwd));
    },
    rebase: async (
      targetBranch: string,
      options?: { cwd?: string; interactive?: boolean; onto?: string; root?: boolean },
    ) => {
      await gitCall(
        'host.git.branch.rebase',
        'branch.rebase',
        withCwd(options?.cwd, {
          targetBranch,
          interactive: options?.interactive ?? false,
          onto: options?.onto,
          root: options?.root ?? false,
        }),
      );
    },
    abortRebase: async (cwd?: string) => {
      await gitCall('host.git.branch.abortRebase', 'branch.abortRebase', withCwd(cwd));
    },
    continueRebase: async (cwd?: string) => {
      await gitCall('host.git.branch.continueRebase', 'branch.continueRebase', withCwd(cwd));
    },
    skipRebase: async (cwd?: string) => {
      await gitCall('host.git.branch.skipRebase', 'branch.skipRebase', withCwd(cwd));
    },
    upstream: async (cwd?: string) => {
      const result = await gitCall<GitUpstreamInfo | null>(
        'host.git.branch.upstream',
        'branch.upstream',
        withCwd(cwd),
      );
      return result && typeof result === 'object' ? result : null;
    },
    needsUpstream: async (cwd?: string) => {
      const result = await gitCall<{ needsUpstream: boolean }>(
        'host.git.branch.needsUpstream',
        'branch.needsUpstream',
        withCwd(cwd),
      );
      return Boolean(result.needsUpstream);
    },
    setUpstream: async (remoteName: string, branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.branch.setUpstream',
        'branch.setUpstream',
        withCwd(cwd, { remote: remoteName, branch: branchName }),
      );
    },
    hasUnpushedCommits: async (cwd?: string) => {
      const result = await gitCall<{ hasUnpushedCommits: boolean }>(
        'host.git.branch.hasUnpushedCommits',
        'branch.hasUnpushedCommits',
        withCwd(cwd),
      );
      return Boolean(result.hasUnpushedCommits);
    },
  };

  // `remote` 单独拆组，便于把 transport 名称和 remote 专属 payload 契约放在一起维护。
  const remote = {
    list: async (cwd?: string) =>
      gitCall<GitRemoteInfo[]>('host.git.remote.list', 'remote.list', withCwd(cwd)),
    exists: async (remoteName: string, cwd?: string) => {
      const result = await gitCall<{ exists: boolean }>(
        'host.git.remote.exists',
        'remote.exists',
        withCwd(cwd, { remote: remoteName }),
      );
      return Boolean(result.exists);
    },
    add: async (name: string, url: string, cwd?: string) => {
      await gitCall('host.git.remote.add', 'remote.add', withCwd(cwd, { name, url }));
    },
    pull: async (remoteName: string, branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.remote.pull',
        'remote.pull',
        withCwd(cwd, { remote: remoteName, branch: branchName }),
      );
    },
    push: async (remoteName: string, branchName: string, cwd?: string) => {
      await gitCall(
        'host.git.remote.push',
        'remote.push',
        withCwd(cwd, { remote: remoteName, branch: branchName }),
      );
    },
    status: async (remoteName: string, cwd?: string) => {
      const result = await gitCall<{ status: string }>(
        'host.git.remote.status',
        'remote.status',
        withCwd(cwd, { remote: remoteName }),
      );
      return String(result.status ?? '');
    },
  };

  // `stage` 这一组故意保持很薄，主要做结果形状转换，真正的 git 语义交给 host service。
  const stage = {
    files: async (cwd?: string) => {
      const result = await gitCall<{ files: string[] }>(
        'host.git.stage.files',
        'stage.files',
        withCwd(cwd),
      );
      return Array.isArray(result.files) ? result.files : [];
    },
    add: async (files: string[], cwd?: string) => {
      await gitCall('host.git.stage.add', 'stage.add', withCwd(cwd, { files }));
    },
    addAll: async (cwd?: string) => {
      await gitCall('host.git.stage.addAll', 'stage.addAll', withCwd(cwd));
    },
    reset: async (file: string, cwd?: string) => {
      await gitCall('host.git.stage.reset', 'stage.reset', withCwd(cwd, { file }));
    },
    diff: async (cwd?: string) => {
      const result = await gitCall<{ diff: string }>(
        'host.git.stage.diff',
        'stage.diff',
        withCwd(cwd),
      );
      return String(result.diff ?? '');
    },
  };

  // `git` 子组更贴近底层 service 形状。
  // 其中 `status` 直接复用 workspace.status，保证两个入口行为一致。
  const gitRoot = {
    init: async (cwd?: string) => {
      await gitCall('host.git.git.init', 'git.init', withCwd(cwd));
    },
    isInstalled: async () => {
      const result = await gitCall<{ installed: boolean }>(
        'host.git.git.isInstalled',
        'git.isInstalled',
      );
      return Boolean(result.installed);
    },
    version: async () => {
      const result = await gitCall<{ version: string }>('host.git.git.version', 'git.version');
      return String(result.version ?? '');
    },
    isInit: async (cwd?: string) => {
      const result = await gitCall<{ isInit: boolean }>(
        'host.git.git.isInit',
        'git.isInit',
        withCwd(cwd),
      );
      return Boolean(result.isInit);
    },
    clone: async (repoUrl: string, targetDir?: string, cwd?: string) => {
      await gitCall('host.git.git.clone', 'git.clone', withCwd(cwd, { repoUrl, targetDir }));
    },
    status: workspace.status,
  };

  // user/tag 访问频率不高，但语义和 workspace/branch 明显不同，因此单独成组。
  const user = {
    get: async (cwd?: string) =>
      gitCall<GitUserInfo>('host.git.user.get', 'user.get', withCwd(cwd)),
    set: async (name: string, email: string, cwd?: string) => {
      await gitCall('host.git.user.set', 'user.set', withCwd(cwd, { name, email }));
    },
  };

  const tag = {
    list: async (cwd?: string) => {
      const result = await gitCall<{ tags: string[] }>(
        'host.git.tag.list',
        'tag.list',
        withCwd(cwd),
      );
      return Array.isArray(result.tags) ? result.tags : [];
    },
    create: async (
      tagName: string,
      options?: { cwd?: string; annotated?: boolean; message?: string },
    ) => {
      // 是否创建 lightweight/annotated tag 由 host 根据下面两个显式参数决定；
      // 这里的包装只是把这个决策保持在 schema 层可读。
      await gitCall(
        'host.git.tag.create',
        'tag.create',
        withCwd(options?.cwd, {
          tag: tagName,
          annotated: options?.annotated ?? false,
          message: options?.message,
        }),
      );
    },
    delete: async (tagName: string, cwd?: string) => {
      await gitCall('host.git.tag.delete', 'tag.delete', withCwd(cwd, { tag: tagName }));
    },
  };

  // `plan.*` 是纯命令规划器：永远不直接改仓库状态，只返回归一化后的 exec 命令快照。
  const plan = {
    addAll: async () => gitCommand('host.git.plan.addAll', 'plan.addAll'),
    init: async () => gitCommand('host.git.plan.init', 'plan.init'),
    commitMessage: async (message: string) =>
      gitCommand('host.git.plan.commitMessage', 'plan.commitMessage', { message }),
    commitAmend: async (message?: string, noEdit?: boolean) =>
      gitCommand('host.git.plan.commitAmend', 'plan.commitAmend', {
        message,
        noEdit: noEdit ?? false,
      }),
    push: async (remoteName: string, branchName: string) =>
      gitCommand('host.git.plan.push', 'plan.push', { remote: remoteName, branch: branchName }),
    pushTag: async (remoteName: string, tagName: string) =>
      gitCommand('host.git.plan.pushTag', 'plan.pushTag', { remote: remoteName, tag: tagName }),
    tagCreateLightweight: async (tagName: string) =>
      gitCommand('host.git.plan.tagCreateLightweight', 'plan.tagCreateLightweight', {
        tag: tagName,
      }),
    tagCreateAnnotated: async (tagName: string, message: string) =>
      gitCommand('host.git.plan.tagCreateAnnotated', 'plan.tagCreateAnnotated', {
        tag: tagName,
        message,
      }),
    tagDelete: async (tagName: string) =>
      gitCommand('host.git.plan.tagDelete', 'plan.tagDelete', { tag: tagName }),
  };

  // 顶层快捷方法都只是分组实现的别名。
  // 它们存在的目的只是兼容旧 schema，不应被视为第二套行为来源。
  return {
    status: workspace.status,
    isClean: workspace.isClean,
    changedFiles: workspace.changedFiles,
    remotes: remote.list,
    remoteExists: remote.exists,
    commit: workspace.commit,
    lastCommitMessage: workspace.lastCommitMessage,
    lastCommitHash: workspace.lastCommitHash,
    commitLog: workspace.commitLog,
    command: async (args) => gitCommand('host.git.command', 'command', { args }),
    git: gitRoot,
    branch,
    remote,
    stage,
    workspace,
    user,
    tag,
    plan,
  };
}
