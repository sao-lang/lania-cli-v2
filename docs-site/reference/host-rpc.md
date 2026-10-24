# Host RPC 方法目录

<div class="lead">
Host RPC 是 Node Bridge 反向调用 Rust 宿主的通道。ctx.tools 中 exec、fs、git、pm、log、tasks、progress、interaction 等命名空间最终都会落到这些 host.* 方法上。
</div>

## 调用链

```text
ctx.tools.exec.run(...)
  → hostCall('host.exec.run', params)
  → stdout: host_request
  → Rust HostRpcAdapter::dispatch
  → 领域 handler
  → stdin: host_response
  → ctx.tools.exec.run 返回 { result, events }
```

协议 envelope 见 [Rust ↔ Node 通信协议](/architecture/bridge-protocol)，使用侧 API 见 [ctx.tools API](/reference/host-tools)。

## 请求与响应

请求：

```json
{
  "type": "host_request",
  "payload": {
    "id": "host-req-18f...-1",
    "method": "host.fs.write",
    "params": {
      "cwd": "/project",
      "path": "src/generated.ts",
      "content": "export {}",
      "mkdirp": true
    }
  }
}
```

响应：

```json
{
  "type": "host_response",
  "payload": {
    "id": "host-req-18f...-1",
    "result": { "ok": true },
    "error": null,
    "events": []
  }
}
```

失败：

```json
{
  "type": "host_response",
  "payload": {
    "id": "host-req-18f...-1",
    "result": null,
    "error": {
      "code": "E_HOST_RPC",
      "message": "failed to read /project/missing.ts: ..."
    },
    "events": []
  }
}
```

Node 侧收到错误后会抛出 `Error("[E_HOST_RPC] ...")`；超时是 `E_HOST_TIMEOUT`。

## 公共参数

| 字段                               | 类型    | 说明                             |
| ---------------------------------- | ------- | -------------------------------- |
| `cwd`                              | string  | 相对路径的解析基准               |
| `__toolsPolicy`                    | object  | Node 侧策略快照，Rust 会再次执行 |
| `__toolsPolicy.exec.allowShell`    | boolean | 是否允许 shell                   |
| `__toolsPolicy.exec.allowEnvWrite` | boolean | 是否允许注入 env                 |
| `__toolsPolicy.fs.writeRoot`       | string  | 写操作允许的根目录               |

`ctx.tools.host.call` 会自动注入 `cwd` 和 `__toolsPolicy`。直接构造 `host_request` 时不会自动注入，Rust 会按缺省策略处理。

## host.exec.\*

| 方法                   | 参数                                                                              | 返回                                                            |
| ---------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| `host.exec.run`        | `program`, `args?`, `cwd?`, `env?`, `timeoutMs?`, `killProcessTree?`, `useShell?` | `{ exitCode, stdout, stderr, skipped?, timedOut?, cancelled? }` |
| `host.exec.runChecked` | 同上                                                                              | 同上；非零退出码时返回错误                                      |
| `host.exec.shell`      | `script`, `cwd?`, `timeoutMs?`, `checked?`                                        | `{ exitCode, stdout, stderr }`                                  |
| `host.exec.history`    | `cwd?`                                                                            | `Array<{ program, args, cwd, env, useShell }>`                  |

策略：

- `host.exec.shell` 需要 `exec.allowShell !== false`。
- `host.exec.run` 的 `useShell=true` 需要 `exec.allowShell !== false`。
- 任何非空 `env` 需要 `exec.allowEnvWrite !== false`。

```ts
const result = await ctx.tools.host.call('host.exec.runChecked', {
  program: 'git',
  args: ['status', '--porcelain'],
  cwd: ctx.cwd,
  timeoutMs: 10_000,
});
```

## host.fs.\*

| 方法              | 参数                                    | 返回                                |
| ----------------- | --------------------------------------- | ----------------------------------- |
| `host.fs.exists`  | `path`                                  | `{ exists }`                        |
| `host.fs.read`    | `path`                                  | `{ content }`                       |
| `host.fs.write`   | `path`, `content`, `append?`, `mkdirp?` | `{ ok: true }`                      |
| `host.fs.mkdirp`  | `path`                                  | `{ ok: true }`                      |
| `host.fs.remove`  | `path`, `recursive?`                    | `{ ok: true, removed }`             |
| `host.fs.readdir` | `path`                                  | `{ entries: string[] }`             |
| `host.fs.stat`    | `path`                                  | `{ isFile, isDir, size, mtimeMs? }` |

`host.fs.write`、`host.fs.mkdirp`、`host.fs.remove` 会校验 `fs.writeRoot`。读、列目录和 stat 不做写根限制。

## host.git.\*

Git 域方法最多，按子域分组。所有方法都接受可选 `cwd`。

### 顶层

`host.git.status`、`host.git.isClean`、`host.git.changedFiles`、`host.git.remotes`、`host.git.remoteExists`、`host.git.commit`、`host.git.lastCommitMessage`、`host.git.lastCommitHash`、`host.git.commitLog`、`host.git.command`

### 仓库

`host.git.git.init`、`host.git.git.isInstalled`、`host.git.git.version`、`host.git.git.isInit`、`host.git.git.clone`、`host.git.git.status`

### 分支

`host.git.branch.current`、`listLocal`、`listRemote`、`listAll`、`exists`、`existsLocal`、`existsRemote`、`create`、`switch`、`delete`、`merge`、`mergeWithOptions`、`mergeNoFF`、`abortMerge`、`cherryPick`、`continueCherryPick`、`abortCherryPick`、`rebase`、`abortRebase`、`continueRebase`、`skipRebase`、`upstream`、`needsUpstream`、`setUpstream`、`hasUnpushedCommits`

### 远端

`host.git.remote.list`、`exists`、`add`、`pull`、`push`、`status`

### 暂存区

`host.git.stage.files`、`add`、`addAll`、`reset`、`diff`

### 工作区

`host.git.workspace.status`、`statusPorcelain`、`changedFiles`、`isClean`、`hasChanges`、`commit`、`commitAmend`、`lastCommitMessage`、`lastCommitHash`、`commitFiles`、`commitLog`、`revert`、`abortRevert`、`continueRevert`

### 用户、标签、命令计划

- `host.git.user.get` / `set`
- `host.git.tag.list` / `create` / `delete`
- `host.git.plan.addAll` / `init` / `commitMessage` / `commitAmend` / `push` / `pushTag` / `tagCreateLightweight` / `tagCreateAnnotated` / `tagDelete`

`plan.*` 返回可执行命令快照，不直接执行 Git。

## host.pm.\*

| 方法                                    | 参数                                                    | 返回                     |
| --------------------------------------- | ------------------------------------------------------- | ------------------------ |
| `host.pm.detect`                        | `cwd?`, `language?`                                     | `{ manager, language }`  |
| `host.pm.spec`                          | `manager?`, `cwd?`, `language?`                         | 包管理器 spec 对象       |
| `host.pm.supportedManagers`             | —                                                       | `{ managers: string[] }` |
| `host.pm.loadPackageJsonSnapshot`       | `cwd?`                                                  | package.json 快照        |
| `host.pm.scriptExists`                  | `script`, `cwd?`                                        | `{ exists }`             |
| `host.pm.command.init`                  | `manager?`, `cwd?`, `language?`                         | `{ program, args }`      |
| `host.pm.command.installAll`            | `manager?`, `cwd?`, `language?`                         | `{ program, args }`      |
| `host.pm.command.install`               | `packages`, `dev?`, `manager?`, `cwd?`, `language?`     | `{ program, args }`      |
| `host.pm.command.remove`                | `packages`, `manager?`, `cwd?`, `language?`             | `{ program, args }`      |
| `host.pm.command.update`                | `packages`, `manager?`, `cwd?`, `language?`             | `{ program, args }`      |
| `host.pm.command.runScript`             | `script`, `args?`, `checked?`, `manager?`, `cwd?`       | `{ program, args }`      |
| `host.pm.command.publish`               | `tag?`, `manager?`, `cwd?`, `language?`                 | `{ program, args }`      |
| `host.pm.command.addDependencyCommands` | `dependencies?`, `devDependencies?`, `manager?`, `cwd?` | `{ commands: [...] }`    |

支持 `manager`：`npm`、`pnpm`、`yarn`、`bun`、`pip`、`uv`、`poetry`、`cargo`、`go`、`pub`、`bundler`、`composer`。未显式指定时按 `cwd` / `language` 探测。

注意：`host.pm.command.*` 只做命令规划，不执行。执行仍走 `host.exec.*`。

## host.runtime.\*

| 方法                  | 参数                    | 返回                                                                                                                 |
| --------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `host.runtime.list`   | —                       | `{ runtimes: RuntimeSpec[] }`                                                                                        |
| `host.runtime.detect` | `file`, `cwd?`          | `{ file, language, runtime }`                                                                                        |
| `host.runtime.spec`   | `language`              | `RuntimeSpec`                                                                                                        |
| `host.runtime.plan`   | `file`, `args?`, `cwd?` | `{ runtime, language, program, args, cwd, preparedCommand?, preparedSummary? }`                                      |
| `host.runtime.run`    | `file`, `args?`, `cwd?` | `{ file, runtime, language, program, args, cwd, preparedCommand?, preparedSummary?, exitCode, stdout, stderr, ... }` |

`plan` 只生成命令计划；`run` 在 Rust 侧执行，编译型语言会先 prepare、再运行临时二进制并清理产物。未知 language 返回 `unknown runtime language: <language>`。

## host.log.\*

| 方法               | 参数                                                               | 返回                  |
| ------------------ | ------------------------------------------------------------------ | --------------------- |
| `host.log.emit`    | `level?`, `message`, `target?`, `traceId?`, `phase?`, `operation?` | `{ ok: true }`        |
| `host.log.entries` | —                                                                  | 日志条目数组          |
| `host.log.clear`   | —                                                                  | `{ ok: true }`        |
| `host.log.ascii`   | `message`                                                          | `{ lines: string[] }` |

`level` 支持 `trace`、`debug`、`info`、`warn`、`error`，其他值按 `info` 处理。

## host.tasks.\*

| 方法                  | 参数                                 | 返回                      |
| --------------------- | ------------------------------------ | ------------------------- |
| `host.tasks.register` | `id`, `title`, `group?`, `priority?` | `{ ok: true }`            |
| `host.tasks.start`    | `id`, `title?`                       | `{ ok: true }`            |
| `host.tasks.update`   | `id`, `detail?`                      | `{ ok: true }`            |
| `host.tasks.complete` | `id`, `detail?`                      | `{ ok: true }`            |
| `host.tasks.fail`     | `id`, `detail?`                      | `{ ok: true }`            |
| `host.tasks.cancel`   | `id`, `detail?`                      | `{ ok: true }`            |
| `host.tasks.snapshot` | —                                    | `{ tasks: TaskRecord[] }` |
| `host.tasks.events`   | —                                    | `{ events: TaskEvent[] }` |

## host.progress.\*

`begin`、`beginGroup`、`beginStep`、`beginItem`、`advance`、`updateTotal`、`message`、`detail`、`linkTask`、`finish`、`fail`、`cancel`、`reset`、`resetAll`、`completeAll`、`failAll`、`snapshot`、`events`、`summary`、`render`、`contains`、`suspendTerminal`、`resumeTerminal`

公共参数：

- `id`：进度项 id。
- `total`：总数。
- `parentId`：父级 id，用于 step/item。
- `kind`：`spinner`、`progress_bar`、`static_step`。
- `detail` / `message`：文本。
- `delta`：推进量，默认 1。

## host.interaction.\*

| 方法                                | 参数                                                                                       | 返回                    |
| ----------------------------------- | ------------------------------------------------------------------------------------------ | ----------------------- |
| `host.interaction.input`            | `field` / `id`, `message`, `defaultValue?`, `context?`, `answers?`, `fallback?`, `locale?` | `{ answer }`            |
| `host.interaction.confirm`          | 同上                                                                                       | `{ answer: boolean }`   |
| `host.interaction.select`           | 同上 + `choices`                                                                           | `{ answer }`            |
| `host.interaction.multiSelect`      | 同上 + `choices`                                                                           | `{ answer: unknown[] }` |
| `host.interaction.password`         | 同上                                                                                       | `{ answer: string }`    |
| `host.interaction.editor`           | 同上                                                                                       | `{ answer: string }`    |
| `host.interaction.prompt`           | `questions`, `options?`                                                                    | `PromptState`           |
| `host.interaction.flow.execute`     | `questions`, `options?`                                                                    | `PromptState`           |
| `host.interaction.resetAccumulated` | —                                                                                          | `{ ok: true }`          |

`fallback` 支持 `{ type: 'use_default' }`、`{ type: 'use_value', value }`、`{ type: 'skip' }`、`{ type: 'error' }`。CI 环境可以通过 `LANIA_PROMPT_ANSWERS_JSON` 预置答案。

## 错误与审计

| 场景                | 结果                                            |
| ------------------- | ----------------------------------------------- |
| transport 未安装    | Node 抛 `host rpc transport is not installed`   |
| 超过 30s            | `[E_HOST_TIMEOUT]`                              |
| Rust handler 不存在 | `[E_HOST_RPC] unsupported host rpc method: ...` |
| Rust 执行失败       | `[E_HOST_RPC] ...`                              |
| 策略拒绝            | `[E_HOST_RPC] ... E_TOOLS_DENIED ...`           |

Rust 每次 Host RPC 都会写一条 `tool_call` 审计日志，包含 `tool`、`method`、`cwd`、`traceId`、`durationMs`、`ok`。

## 下一步

- [Bridge 通信协议](/architecture/bridge-protocol)
- [ctx.tools API](/reference/host-tools)
- [Bridge 方法目录](/reference/bridge-methods)
