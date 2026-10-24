# ctx.tools API

<div class="lead">
ctx.tools 是动态命令 handler 与 inline hook 的宿主能力对象。它把 Node 本地能力、Bridge 能力和 Rust Host RPC 能力统一挂到同一个命名空间下，并在调用边界做策略校验与审计。
</div>

## ctx.tools 从哪里来

```text
command.invokeDynamic
  → prepareDynamicCommandExecution()
  → createSchemaTools({ cwd, traceId, mount, path, handlerId, events })
  → DynamicCommandContext.tools
```

inline hook 也会拿到类似的 tools 视图。它不是全局单例，而是**一次执行一份**。

## 命名空间总览

| 命名空间      | 能力                          | 是否受 `tools.allow` 约束                 |
| ------------- | ----------------------------- | ----------------------------------------- |
| `config`      | 读取 lan.config / 工具配置    | 是                                        |
| `bridge`      | 调用 Node 内建 bridge 能力    | 是（`bridge.allowMethods` 额外限制）      |
| `compiler`    | `bridge.compiler` 的语义别名  | 是                                        |
| `lint`        | `bridge.lint` 的语义别名      | 是                                        |
| `path`        | 路径处理                      | 否                                        |
| `workspace`   | 工作区、package.json、配置    | 是                                        |
| `env`         | 环境变量与平台信息            | 否                                        |
| `json`        | JSON 读写与 patch             | 是                                        |
| `result`      | 构造标准结果/事件             | 否                                        |
| `text`        | ANSI 文本样式                 | 否                                        |
| `host`        | 直接调用 Host RPC             | 是                                        |
| `exec`        | 执行命令/shell                | 是（`exec.allowShell` / `allowEnvWrite`） |
| `git`         | Git 查询与操作                | 是                                        |
| `pm`          | 包管理器检测与命令规划        | 是                                        |
| `runtime`     | 脚本运行时识别、规划与执行    | 是                                        |
| `scaffold`    | 模板层、依赖计划、merge/guard | 内部依赖 `pm`                             |
| `fs`          | 文件读写、目录、glob          | 是（`fs.writeRoot`）                      |
| `log`         | 日志 emit / entries           | 是                                        |
| `tasks`       | 任务执行器                    | 是                                        |
| `progress`    | 进度条                        | 是                                        |
| `interaction` | 交互补问                      | 是                                        |

## tools 策略

`lan.config.*` 中：

```ts
export default {
  tools: {
    allow: ['git', 'pm', 'exec', 'fs', 'log', 'text', 'bridge', 'workspace', 'path', 'result'],
    deny: ['interaction'],
    exec: {
      allowShell: false,
      allowEnvWrite: false,
    },
    fs: {
      writeRoot: '.',
    },
    bridge: {
      allowMethods: ['bridge.*', 'config.*', 'compiler.*', 'lint.*', 'commit*'],
    },
  },
};
```

规则：

- `allow` 为空/缺省 → 不额外限制。
- `allow` 非空 → 只有列出的命名空间可用。
- `deny` 优先于 `allow`。
- `bridge.allowMethods` 支持 `*` 通配。
- `exec.allowShell` 默认 true；设为 false 时拒绝 `shell`/`useShell`。
- `exec.allowEnvWrite` 默认 true；设为 false 时拒绝带 `env` 的调用。
- `fs.writeRoot` 默认 `.`；所有写操作必须落在该目录内。
- 策略错误统一为 `[E_TOOLS_DENIED] ...`。

Rust 侧会再次执行 `exec/fs` 策略，防止绕过。

## config

```ts
config.asRecord(value)
config.loadLan(options?: { cwd?, searchFrom?, candidates? })
config.loadTool(tool, cwd?)
config.findFirstExisting(cwd, candidates)
config.fileExists(path)
config.loadModule(path)
config.resolveModuleFromCwd(specifier, cwd?)
config.bridgePackageDir()
config.normalizeError(error)
config.supportedTypes()
config.searchPlaces(type)
config.search(typeOrModule, from?)
config.load(typeOrModule, configPath?, options?)
```

## bridge

### 请求构造

```ts
bridge.request(method, params?)
bridge.handshakeRequest()
bridge.pingRequest()
bridge.shutdownRequest()
bridge.metricsRequest(cwd?)
bridge.subscribeRequest(cwd?)
bridge.loadLanConfigRequest(cwd?)
bridge.loadToolConfigRequest(cwd, tool)
bridge.compilerDevRequest(cwd?, port?)
bridge.compilerBuildRequest(cwd?, watch?)
bridge.compilerBuildWithOptionsRequest(cwd?, watch?, mode?, outputDir?)
bridge.compilerStopRequest()
bridge.lintRunRequest(cwd?, fix?, concurrency?)
bridge.systemListCommandsRequest(cwd?, options?)
bridge.commitizenRunRequest(cwd?, kind, scope?, subject?)
bridge.commitlintRunRequest(cwd?, message)
```

### 调用

```ts
bridge.call(requestOrMethod, params?)
bridge.callAsync(requestOrMethod, params?)
bridge.openCall(requestOrMethod, params?)
bridge.raw.request(method, params?)
bridge.raw.call(method, params?)
```

`bridge` 在 Node 进程内分发以下方法：

- `bridge.*`
- `config.*`
- `compiler.*`
- `lint.*`
- `system.*`
- `commitizen.*`
- `commitlint.*`

其他方法（例如 `product.*`、`template.*`）不在本地分发范围内，会报：

```text
unsupported bridge method in schema tools: <method>
```

### 语义化入口

```ts
bridge.config.loadLan(cwd?)
bridge.config.loadTool(tool, cwd?)
bridge.compiler.dev(options?)
bridge.compiler.build(options?)
bridge.compiler.stop()
bridge.lint.run(options?)
bridge.system.listCommands(options?)
bridge.commit.commitizen(options)
bridge.commit.commitlint(message, options?)
```

### 事件与指标

```ts
bridge.subscribeEvents(callback); // 返回取消订阅函数
bridge.metricsSnapshot();
bridge.supportedEvents();
bridge.usingProcessTransport();
bridge.timeout();
bridge.shutdown();
bridge.shutdownAsync();
```

## compiler / lint

`compiler` 与 `lint` 是 `bridge.compiler` / `bridge.lint` 的别名：

```ts
compiler.dev({ cwd, port, host, hmr, mode });
compiler.build({ cwd, watch, mode, outputDir });
compiler.stop();

lint.run({ cwd, fix, mode, concurrency, linters, groupedOutput });
```

## path

```ts
path.join(...parts)
path.resolve(...parts)
path.dirname(p)
path.basename(p, ext?)
path.relative(from, to)
path.extname(p)
path.normalize(p)
path.isAbsolute(p)
```

## workspace

```ts
workspace.cwd()
workspace.root()
workspace.gitRoot()
workspace.packageJson(cwd?)
workspace.hasFile(path)
workspace.findFile(candidates, cwd?)
workspace.detectPackageManager(cwd?)
workspace.loadLanConfig(cwd?)
workspace.loadToolConfig(tool, cwd?)
```

`detectPackageManager` 返回：

- `pnpm`
- `yarn`
- `npm`
- `unknown`

## env

```ts
env.cwd()
env.get(name, defaultValue?)
env.has(name)
env.all()
env.platform()
env.isCI()
env.home()
env.tmpdir()
```

## json

```ts
json.parse(text)
json.stringify(value, space?)
json.read(path)
json.write(path, value, { space? })
json.patch(path, patch)
```

## result

```ts
result.ok(data?, { exitCode? })
result.fail(message, options?)
result.withEvents(result, events)
result.withExitCode(result, exitCode)
result.event(method, params)
```

标准成功结果：

```ts
return ctx.tools.result.ok({ hello: 'world' });
```

带事件：

```ts
return ctx.tools.result.withEvents(ctx.tools.result.ok({ ok: true }), [
  { method: 'event.log', params: { level: 'info', message: 'done' } },
]);
```

## text

```ts
text.style(content, options?)
text.render(content, options?)
text.rgb(r, g, b)
text.hsl(h, s, l)
```

`style()` 返回可链式调用的句柄：

```ts
ctx.tools.text
  .style('Lania', { color: ctx.tools.text.rgb(66, 184, 131) })
  .bold()
  .underline()
  .render();
```

支持的修饰：

- `bold`
- `italic`
- `underline`
- `overline`
- `inverse`
- `strikethrough`
- `hidden`
- `visible`

## host

```ts
host.call<T>(method, params?, options?: { timeoutMs? })
```

`host.call` 会自动：

- 校验 `host.<tool>.*` 是否被 `tools.allow` 允许
- 注入 `cwd`
- 注入 `__toolsPolicy`（exec/fs 子策略）
- 默认 30s 超时

完整的方法名、参数和返回结构见 [Host RPC 方法目录](/reference/host-rpc)。

示例：

```ts
const result = await ctx.tools.host.call('host.fs.stat', {
  path: 'package.json',
});
```

## exec

```ts
exec.workingDir()
exec.command(program)
exec.shell(script)
exec.run(command)
exec.runWithOptions(command, options)
exec.runChecked(command)
exec.runCheckedWithOptions(command, options)
exec.history()
exec.bash(script, options?)
exec.bashChecked(script, options?)
exec.spawn(program, args?, options?)
exec.spawnChecked(program, args?, options?)
```

命令构造器：

```ts
const cmd = ctx.tools.exec
  .command('git')
  .withArgs(['status', '--porcelain'])
  .inDir(ctx.cwd)
  .withEnv('CI', '1');

const result = await cmd.run({ timeoutMs: 10_000 });
```

`run` 返回：

```ts
{
  exitCode: number
  stdout: string
  stderr: string
  skipped?: boolean
  timedOut?: boolean
  cancelled?: boolean
}
```

`runChecked` 在非零退出码时抛错。

## git

### 查询

```ts
git.status(cwd?)
git.isClean(cwd?)
git.changedFiles(cwd?)
git.remotes(cwd?)
git.remoteExists(remote, cwd?)
git.lastCommitMessage(cwd?)
git.lastCommitHash(cwd?)
git.commitLog(options?)
```

### 修改

```ts
git.commit(message, cwd?)
git.command(args)
```

### 命名空间

```ts
git.branch.{current,create,delete,exists,listAll,listLocal,listRemote,switch,upstream,...}
git.remote.{add,list,exists,pull,push,status}
git.stage.{add,addAll,diff,files,reset}
git.workspace.{status,isClean,changedFiles,commit,commitAmend,commitFiles,...}
git.user.{get,set}
git.tag.{create,delete,list}
git.plan.{addAll,commitAmend,commitMessage,init,push,pushTag,tagCreateAnnotated,tagCreateLightweight,tagDelete}
```

## pm

```ts
pm.detectFromFiles(files)
pm.detect(cwd?, { language? })
pm.supportedManagers()
pm.spec(managerOrCwd?, { language? })
pm.binary(manager)
pm.lockfile(manager)
pm.lockfileStrategy(manager)
pm.loadPackageJsonSnapshot(cwd?)
pm.scriptExists(script, cwd?)
pm.requireScript(script, cwd?)
pm.init({ manager?, cwd?, language? })
pm.installAll({ manager?, cwd?, language? })
pm.install(packages, options?)
pm.remove(packages, options?)
pm.update(packages, options?)
pm.run(script, extraArgs?, options?)
pm.publish({ manager?, tag?, cwd?, language? })
pm.command.init(manager?)
pm.command.installAll(manager?)
pm.command.install(packages, options?)
pm.command.remove(packages, options?)
pm.command.update(packages, options?)
pm.command.runScript(script, options?)
pm.command.runScriptChecked(script, options?)
pm.command.publish(options?)
pm.command.addDependencyCommands(options?)
```

支持的 manager：`npm`、`pnpm`、`yarn`、`bun`、`pip`、`uv`、`poetry`、`cargo`、`go`、`pub`、`bundler`、`composer`。所有 `command.*` 规划由 Rust `lania-pm` 生成，Node 侧不保留第二份命令拼装逻辑。

## runtime

```ts
runtime.list()
runtime.detect(file, cwd?)
runtime.spec(language)
runtime.plan(file, { cwd?, args? })
runtime.run(file, { cwd?, args? })
```

- `detect` 优先 shebang，其次扩展名；返回 `{ file, language, runtime }`。
- `plan` 只返回命令快照：`{ runtime, language, program, args, cwd, preparedCommand?, preparedSummary? }`。
- `run` 在 Rust 侧执行；编译型语言会先 prepare，再运行临时二进制并清理产物。
- `runtime.run` 受 `tools.allow` 的 `runtime` 命名空间约束。

## scaffold

```ts
scaffold.currentPlan()
scaffold.renderTemplateLayers(input?)
scaffold.dependencyPlan(input?)
```

`renderTemplateLayers` 返回：

```ts
{
  templates: string[]
  files: Array<{ path: string; content: string }>
  collisions: string[]
}
```

`dependencyPlan` 返回：

```ts
{
  manager: string
  templates: string[]
  dependencies: string[]
  devDependencies: string[]
  scripts: Record<string, string>
  packageJsonPatch: { ... }
  installCommands: Array<{ program: string; args: string[] }>
}
```

## fs

```ts
fs.exists(path)
fs.read(path)
fs.readJson(path)
fs.write(path, content, { append?, mkdirp? })
fs.writeJson(path, value, { space?, mkdirp? })
fs.append(path, content, { mkdirp? })
fs.mkdir(path, { recursive? })
fs.mkdirp(path)
fs.remove(path, { recursive? })
fs.copy(from, to, { overwrite? })
fs.move(from, to, { overwrite? })
fs.readdir(path)
fs.stat(path)
fs.glob(pattern, options?)
fs.ensureFile(path, content?)
fs.ensureDir(path)
fs.replace(path, matcher, replacer)
```

写操作受 `fs.writeRoot` 限制。

## log

```ts
log.log(message, { target? })
log.emit(level, message, options?)
log.trace(message, { target? })
log.debug(message, { target? })
log.info(message, { target? })
log.warn(message, { target? })
log.error(message, { target? })
log.success(message, { target? })
log.scoped(scope)
log.entries()
log.clear()
log.ascii(message)
```

## tasks

```ts
tasks.create(options?)
tasks.run(tasks?, options?)
tasks.register(id, title, options?)
tasks.start(id, title?)
tasks.update(id, detail)
tasks.complete(id, detail?)
tasks.fail(id, detail?)
tasks.cancel(id, detail?)
tasks.snapshot()
tasks.events()
```

任务执行器：

```ts
const executor = ctx.tools.tasks.create({ stopOnError: true });
executor
  .add({ id: 'a', title: 'A', run: async () => 1 })
  .add({ id: 'b', title: 'B', run: async () => 2 });

const results = await executor.run();
```

## progress

```ts
progress.begin(id, total?)
progress.beginGroup(...)
progress.beginStep(...)
progress.beginItem(...)
progress.advance(id, delta?)
progress.updateTotal(id, total?)
progress.message(id, message)
progress.detail(id, detail)
progress.linkTask(id, taskId)
progress.finish(id)
progress.fail(id, detail?)
progress.cancel(id, detail?)
progress.reset(id)
progress.resetAll()
progress.completeAll()
progress.failAll(detail?)
progress.snapshot()
progress.events()
progress.summary()
progress.render(mode?)
progress.contains(id)
progress.onProgress(callback)
progress.suspendTerminal()
progress.resumeTerminal()
```

## interaction

```ts
interaction.input(options)
interaction.confirm(options)
interaction.select(options)
interaction.multiSelect(options)
interaction.password(options)
interaction.editor(options)
interaction.prompt(options)
interaction.createFlow(options?)
```

Flow：

```ts
const flow = ctx.tools.interaction.createFlow()
flow
  .addQuestion({ field: 'name', message: 'Name?', kind: 'input' })
  .addQuestion({ field: 'env', message: 'Env?', kind: 'select', choices: [...] })

const state = await flow.execute()
```

## 审计事件

每次工具调用都会向当前执行的 events 追加上：

```json
{
  "method": "event.log",
  "params": {
    "level": "debug",
    "target": "schema.tools",
    "message": "ok tools.fs.write",
    "operation": "tools.fs.write",
    "tool": "fs",
    "methodName": "write",
    "durationMs": 3,
    "ok": true
  }
}
```

失败时 `level=warn`，并带有 `error`。

## 下一步

- [ctx.tools 使用指南](/guide/host-tools)
- [Bridge 方法目录](/reference/bridge-methods)
- [Rust ↔ Node 通信协议](/architecture/bridge-protocol)
