# 模板与脚手架

<div class="lead">
create、add、template 三个命令共同构成 Lania CLI 的脚手架能力：template 查询模板，create 初始化项目，add 向已有项目补充单个文件或配置。
</div>

## template

### 列出模板

```bash
lan template
```

返回模板元信息，包括：

- `name`
- `title`
- `legacyTemplateDir`
- `migrationLayer`
- `ownership`
- `renderEngine`
- `schemaVersion`
- `useCases`

### 查看详情

```bash
lan template spa-vue
lan template toolkit
```

## create

### 基本用法

```bash
lan create [path] [options]
```

### 常用选项

| 选项                         | 说明                       |
| ---------------------------- | -------------------------- |
| `-n, --name <name>`          | 项目名                     |
| `-t, --template <template>`  | 模板名                     |
| `-p, --package-manager <pm>` | npm / pnpm / yarn / bun    |
| `--directory <dir>`          | 旧版路径别名               |
| `-g, --git`                  | 初始化 Git 仓库            |
| `--skip-git`                 | 不初始化 Git               |
| `--skip-install`             | 跳过安装                   |
| `--language <lang>`          | 传给模板上下文             |
| `-d, --dry-run`              | 只规划                     |
| `-P, --preview`              | 预览渲染结果并隐含 dry-run |

### 示例

```bash
# 默认 React 项目
lan create --name demo-app

# Vue 项目
lan create --name demo-app --template spa-vue

# 在当前目录创建
lan create .

# 预览
lan create --template spa-vue --preview

# 不安装依赖
lan create --name demo-app --template spa-vue --skip-install
```

### create 内部流程

```text
CLI 参数
  → Rust 工作流输入
  → 模板补问
  → 获取模板依赖
  → 解析依赖版本
  → 渲染文件计划
  → 冲突检测
  → 格式化
  → 写入事务
  → 安装依赖 / 初始化 Git
```

## add

### 基本用法

```bash
lan add [options]
```

### 常用选项

| 选项                        | 说明             |
| --------------------------- | ---------------- |
| `-n, --name <name>`         | 生成物基础名     |
| `-t, --template <template>` | add 模板         |
| `-d, --target <target>`     | 目标目录或路径   |
| `--filepath <path>`         | 旧版目标路径别名 |
| `-f, --force`               | 覆盖冲突文件     |

### 可用模板

| 模板           | 产物                  |
| -------------- | --------------------- |
| `v2`           | Vue 2 组件            |
| `v3`           | Vue 3 组件            |
| `rcc`          | React 类组件          |
| `rfc`          | React 函数组件        |
| `svelte`       | Svelte 组件           |
| `astro`        | Astro 组件            |
| `prettier`     | `prettier.config.js`  |
| `eslint`       | `eslint.config.js`    |
| `stylelint`    | `stylelint.config.js` |
| `editorconfig` | `.editorconfig`       |
| `gitignore`    | `.gitignore`          |
| `tsconfig`     | `tsconfig.json`       |
| `commitizen`   | `cz.config.js`        |

### 示例

```bash
lan add --name button --template v3
lan add --name card --template rfc --target src/components
lan add --template prettier
lan add --template tsconfig --target .
```

### 语言与样式

`add` 会根据项目上下文读取：

- `language`：`js` / `ts`
- `cssProcessor`：`css` / `sass` / `stylus` / `less`

组件扩展名与样式语言会自动选择。

## 模板运行时

模板资产位于：

```text
ts/packages/templates/
  src/
    add-templates/assets/
    templates/
  dist/
```

- `create` 模板是目录级模板，包含 questions、dependencies、files、outputTasks。
- `add` 模板是单资产模板，给定 name + context 渲染一段内容。

运行时优先读取 `dist` 资产；开发态可回退到 `src`。

## 下一步

- [代码生成](/guide/generate)
- [模板参考](/reference/templates)
- [创建项目实战](/cookbook/create-project)
