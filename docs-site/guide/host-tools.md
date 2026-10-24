# ctx.tools 宿主能力

<div class="lead">
ctx.tools 是运行时命令和 inline hook 访问宿主能力的统一入口。它把“Node 本地能力”和“Rust 宿主能力”封装成同一套命名空间，并通过 tools 策略限制高风险操作。
</div>

## 能力落点

```text
ctx.tools
  ├── Node 本地实现
  │     path / env / text / result / json / workspace / scaffold
  ├── Node Bridge 本地分发
  │     bridge / compiler / lint / config / system / commit
  └── Rust Host RPC
        host / exec / fs / git / pm / runtime / log / tasks / progress / interaction
```

同一份代码不需要关心能力最终落在哪一侧，但排查问题时要看：

- 策略拒绝 → `[E_TOOLS_DENIED]`
- Host RPC 超时 → `E_HOST_TIMEOUT`
- Bridge 方法不支持 → `unsupported bridge method in schema tools`

## 最小示例

```ts
handler: async (ctx) => {
  const pkg = await ctx.tools.workspace.packageJson();
  const manager = await ctx.tools.pm.detect();
  const branch = await ctx.tools.git.branch.current();

  await ctx.tools.log.success(`manager=${manager} branch=${branch}`, {
    target: 'demo.inspect',
  });

  return ctx.tools.result.ok({
    packageName: pkg?.name ?? null,
    manager,
    branch,
  });
};
```

## 配置 tools 策略

```ts
export default {
  extensions: { dynamicCommands: true },
  tools: {
    allow: ['git', 'pm', 'exec', 'fs', 'log', 'text', 'bridge', 'workspace', 'path', 'result'],
    deny: [],
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

### 规则

- `allow` 缺省 → 不额外限制。
- `allow` 非空 → 只有列出的能力可用。
- `deny` 优先于 `allow`。
- `fs.writeRoot` 限制所有写操作。
- `exec.allowShell=false` 拒绝 shell。
- `exec.allowEnvWrite=false` 拒绝带 env 的执行。
- `bridge.allowMethods` 支持 `*` 通配。

### 不受 allow 约束的工具

`path`、`env`、`text`、`result` 是纯本地、低风险工具，不经过 `tools.allow` 校验。

## 读写工作区

```ts
const reportPath = ctx.tools.path.resolve(ctx.cwd, '.lania', 'reports', 'inspect.json');

await ctx.tools.fs.ensureDir(ctx.tools.path.dirname(reportPath));
await ctx.tools.fs.writeJson(reportPath, {
  ok: true,
  at: new Date().toISOString(),
});
```

如果 `fs.writeRoot = '.'`，写入 `../outside.json` 会被拒绝。

## 执行命令

```ts
const result = await ctx.tools.exec.spawn('git', ['status', '--porcelain'], {
  cwd: ctx.cwd,
  timeoutMs: 10_000,
});

if (result.exitCode !== 0) {
  return ctx.tools.result.fail(result.stderr);
}
```

`runChecked` 会在非零退出码时抛错；`run` 只返回结果。

## Git

```ts
const status = await ctx.tools.git.status();
const branch = await ctx.tools.git.branch.current();
const changed = await ctx.tools.git.changedFiles();

await ctx.tools.git.stage.addAll();
await ctx.tools.git.workspace.commit('chore: update');
```

修改性操作会经过 `host.git.*` 路由，最终由 `lania-git` 执行。

## 包管理器

```ts
const manager = await ctx.tools.pm.detect();
const spec = await ctx.tools.pm.spec(manager);
const hasTest = await ctx.tools.pm.scriptExists('test');

if (hasTest) {
  await ctx.tools.pm.run('test');
}
```

## 运行时

`ctx.tools.runtime` 把“用 node / python / go 等环境执行脚本”封装为统一工具：

```ts
const detected = await ctx.tools.runtime.detect('scripts/demo.py');
const plan = await ctx.tools.runtime.plan('scripts/demo.py', { args: ['--verbose'] });
const result = await ctx.tools.runtime.run('scripts/demo.py', { args: ['--verbose'] });
```

- `detect` 优先 shebang，其次扩展名。
- `plan` 只生成命令快照，不执行。
- `run` 在 Rust 侧执行；`rust` / `c` 会先编译到临时文件，执行后清理。
- 运行时与包管理器通过元数据关联，例如 `python` → `pip/uv/poetry`、`rust` → `cargo`。

## 调用 Bridge 能力

```ts
const ping = await ctx.tools.bridge.call('bridge.ping');
const config = await ctx.tools.bridge.config.loadLan();

const build = await ctx.tools.bridge.compiler.build({
  cwd: ctx.cwd,
  mode: 'production',
});
```

`ctx.tools.bridge` 是 Node 进程内分发，不走 Rust 宿主。它支持：

- `bridge.*`
- `config.*`
- `compiler.*`
- `lint.*`
- `system.*`
- `commitizen.*`
- `commitlint.*`

`product.*`、`template.*` 等不在本地分发范围内。

## 调用 Host RPC

```ts
const stat = await ctx.tools.host.call('host.fs.stat', {
  path: 'package.json',
});
```

`host.call` 会自动注入：

- `cwd`
- `__toolsPolicy.exec`
- `__toolsPolicy.fs`

Rust 会再次执行策略，避免 Node 侧绕过。完整方法列表见 [Host RPC 方法目录](/reference/host-rpc)。

## 进度与任务

```ts
const id = 'install';
await ctx.tools.progress.begin(id, 3);
await ctx.tools.progress.message(id, 'installing');

await ctx.tools.progress.advance(id, 1);
await ctx.tools.progress.finish(id);
```

任务执行器：

```ts
const executor = ctx.tools.tasks.create({ stopOnError: true });

executor
  .add({ id: 'lint', title: 'Lint', run: () => ctx.tools.pm.run('lint') })
  .add({ id: 'test', title: 'Test', run: () => ctx.tools.pm.run('test') });

const results = await executor.run();
```

## 交互补问

```ts
const name = await ctx.tools.interaction.input({
  field: 'name',
  message: 'Project name?',
  defaultValue: 'demo',
});

const env = await ctx.tools.interaction.select({
  field: 'env',
  message: 'Environment?',
  choices: [
    { label: 'staging', value: 'staging' },
    { label: 'prod', value: 'prod' },
  ],
});
```

## 文本与结果

```ts
const title = ctx.tools.text
  .style('Lania', { color: ctx.tools.text.rgb(66, 184, 131) })
  .bold()
  .render();

await ctx.tools.log.info(title);

return ctx.tools.result.withEvents(ctx.tools.result.ok({ done: true }), [
  { method: 'event.log', params: { level: 'info', message: 'done' } },
]);
```

## 审计

每次工具调用都会追加一条 `event.log`：

```json
{
  "level": "debug",
  "target": "schema.tools",
  "message": "ok tools.fs.write",
  "operation": "tools.fs.write",
  "tool": "fs",
  "methodName": "write",
  "durationMs": 3,
  "ok": true
}
```

失败时 `level=warn`，并带有 `error`。

## 常见错误

| 错误                                                           | 原因                           |
| -------------------------------------------------------------- | ------------------------------ |
| `[E_TOOLS_DENIED] tools.fs.write is not in config.tools.allow` | `allow` 未包含 `fs`            |
| `[E_TOOLS_DENIED] ... outside writeRoot`                       | 写入越界                       |
| `[E_TOOLS_DENIED] ... allowShell=false`                        | 使用了 shell                   |
| `[E_TOOLS_DENIED] ... allowEnvWrite=false`                     | 向命令注入 env                 |
| `unsupported bridge method in schema tools`                    | 调用了本地 bridge 不支持的方法 |
| `E_HOST_TIMEOUT`                                               | Host RPC 超过 30s              |

完整 API 见 [ctx.tools API](/reference/host-tools)。

## 下一步

- [开发运行时命令](/guide/runtime-commands)
- [运行时清单参考](/reference/manifest)
- [Bridge 方法目录](/reference/bridge-methods)
