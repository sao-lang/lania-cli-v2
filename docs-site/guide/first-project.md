# 创建第一个项目

<div class="lead">
本页从空目录开始，使用模板创建一个 Vue 项目，并完成开发、构建、Lint 与模板补充。
</div>

## 1. 查看可用模板

```bash
lan workspace create template
```

当前内置 10 个模板，覆盖 Node.js、Python、Go、Rust、Dart 五种运行时：

| 模板               | 运行时  | 用途                 |
| ------------------ | ------- | -------------------- |
| `spa-react`        | Node.js | React 单页应用       |
| `spa-vue`          | Node.js | Vue 3 单页应用       |
| `toolkit`          | Node.js | Node 工具库          |
| `toolkit-monorepo` | Node.js | pnpm monorepo 工具库 |
| `nestjs`           | Node.js | NestJS 服务端        |
| `express`          | Node.js | Express 服务端       |
| `go-service`       | Go      | Go 服务 / CLI        |
| `rust-lib`         | Rust    | Rust 库              |
| `python-cli`       | Python  | Python CLI           |
| `dart-cli`         | Dart    | Dart CLI             |

非 Node 模板可配合 `--package-manager go / cargo / pip / uv / poetry / dart / pub` 使用，安装命令会按运行时自动选择。

查看模板详情：

```bash
lan workspace create template spa-vue
```

## 2. 预览生成结果

```bash
lan workspace create --name demo-app --template spa-vue --preview
```

`--preview` 会：

- 渲染模板文件；
- 规划文件写入；
- 隐含 dry-run；
- 不真正创建目标目录。

这一步同时会查询模板依赖版本，因此需要能访问 npm registry，或使用 `--skip-install` 跳过安装阶段。

## 3. 创建项目

```bash
lan workspace create --name demo-app --template spa-vue --package-manager pnpm
cd demo-app
```

如果只想先生成文件、不安装依赖：

```bash
lan workspace create --name demo-app --template spa-vue --skip-install
```

## 4. 启动开发

```bash
lan workspace dev
```

自定义端口：

```bash
lan workspace dev --port 3001
```

## 5. 构建

```bash
lan workspace build
```

生产模式：

```bash
lan workspace build --mode production
```

## 6. Lint

```bash
lan workspace lint
lan workspace lint fix
```

指定适配器：

```bash
lan workspace lint --linters eslint,prettier
```

## 7. 向项目补充模板

在已有项目中添加一个 Vue 3 组件：

```bash
lan workspace add --name button --template v3
```

添加配置文件：

```bash
lan workspace add --template prettier
lan workspace add --template eslint
lan workspace add --template tsconfig
```

`add` 模板支持：

`v2`、`v3`、`rcc`、`rfc`、`svelte`、`astro`、`prettier`、`eslint`、`stylelint`、`editorconfig`、`gitignore`、`tsconfig`、`commitizen`。

## 8. 检查生成结果

```bash
lan tools view ./src/button.vue
lan tools view ./src --tree --files-only
```

## 下一步

- [工具命令 tools](/guide/tools)
- [模板与脚手架](/guide/scaffolding)
- [模板参考](/reference/templates)
