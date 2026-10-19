# 创建第一个项目

<div class="lead">
本页从空目录开始，使用模板创建一个 Vue 项目，并完成开发、构建、Lint 与模板补充。
</div>

## 1. 查看可用模板

```bash
lan template
```

当前内置模板：

| 模板               | 用途            |
| ------------------ | --------------- |
| `spa-react`        | React 单页应用  |
| `spa-vue`          | Vue 单页应用    |
| `toolkit`          | 工具库          |
| `toolkit-monorepo` | Monorepo 工具库 |

查看模板详情：

```bash
lan template spa-vue
```

## 2. 预览生成结果

```bash
lan create --name demo-app --template spa-vue --preview
```

`--preview` 会：

- 渲染模板文件；
- 规划文件写入；
- 隐含 dry-run；
- 不真正创建目标目录。

这一步同时会查询模板依赖版本，因此需要能访问 npm registry，或使用 `--skip-install` 跳过安装阶段。

## 3. 创建项目

```bash
lan create --name demo-app --template spa-vue --package-manager pnpm
cd demo-app
```

如果只想先生成文件、不安装依赖：

```bash
lan create --name demo-app --template spa-vue --skip-install
```

## 4. 启动开发

```bash
lan dev
```

自定义端口：

```bash
lan dev --port 3001
```

## 5. 构建

```bash
lan build
```

生产模式：

```bash
lan build --mode production
```

## 6. Lint

```bash
lan lint
lan lint fix
```

指定适配器：

```bash
lan lint --linters eslint,prettier
```

## 7. 向项目补充模板

在已有项目中添加一个 Vue 3 组件：

```bash
lan add --name button --template v3
```

添加配置文件：

```bash
lan add --template prettier
lan add --template eslint
lan add --template tsconfig
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
- [代码生成](/guide/generate)
