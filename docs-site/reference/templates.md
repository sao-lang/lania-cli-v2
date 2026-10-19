# 模板参考

<div class="lead">
模板分为 create 模板与 add 模板。create 生成完整项目目录，add 生成单个文件或配置片段。
</div>

## create 模板

| 模板               | 用途            | 渲染引擎                         |
| ------------------ | --------------- | -------------------------------- |
| `spa-react`        | React 单页应用  | Rust declarative / Node fallback |
| `spa-vue`          | Vue 单页应用    | Rust declarative / Node fallback |
| `toolkit`          | 工具库          | Node Bridge                      |
| `toolkit-monorepo` | Monorepo 工具库 | Node Bridge                      |

### 模板元信息

```json
{
  "name": "spa-vue",
  "title": "spa-vue",
  "legacyTemplateDir": "packages/templates/spa-vue",
  "migrationLayer": "rust_builtin_template",
  "ownership": "rust_preferred_node_fallback",
  "renderEngine": "rust_declarative",
  "schemaVersion": 1,
  "useCases": ["create", "add"]
}
```

### 模板目录

```text
ts/packages/templates/
  src/
    templates/
      spa-react/
      spa-vue/
      toolkit/
      toolkit-monorepo/
    add-templates/
      assets/
  dist/
```

## add 模板

| 模板           | 产物            | 扩展名/文件名         |
| -------------- | --------------- | --------------------- |
| `v2`           | Vue 2 组件      | `.vue`                |
| `v3`           | Vue 3 组件      | `.vue`                |
| `rcc`          | React 类组件    | `.jsx` / `.tsx`       |
| `rfc`          | React 函数组件  | `.jsx` / `.tsx`       |
| `svelte`       | Svelte 组件     | `.svelte`             |
| `astro`        | Astro 组件      | `.astro`              |
| `prettier`     | Prettier 配置   | `prettier.config.js`  |
| `eslint`       | ESLint 配置     | `eslint.config.js`    |
| `stylelint`    | Stylelint 配置  | `stylelint.config.js` |
| `editorconfig` | EditorConfig    | `.editorconfig`       |
| `gitignore`    | Git ignore      | `.gitignore`          |
| `tsconfig`     | TypeScript 配置 | `tsconfig.json`       |
| `commitizen`   | Commitizen 配置 | `cz.config.js`        |

## add 模板上下文

| 字段           | 说明                               |
| -------------- | ---------------------------------- |
| `projectName`  | 项目名，作为组件/变量基础名        |
| `language`     | `js` / `ts`                        |
| `cssProcessor` | `css` / `sass` / `stylus` / `less` |

## v3 模板产物

```vue
<template>
  <div></div>
</template>

<script setup lang="ts"></script>

<style scoped lang="scss"></style>
```

`lang` 与样式处理器会根据上下文自动选择。

## 模板资产加载顺序

1. `dist/add-templates/assets/*.ejs`
2. `src/add-templates/assets/*.ejs`（开发态回退）

## 自定义产品模板

产品 CLI 可以通过 `product.templatesDir` 指向自己的模板目录：

```json
{
  "product": {
    "templatesDir": "./product/templates"
  }
}
```

模板元信息通常包含：

```json
{
  "id": "base"
}
```

## 模板目录协议

每个 create 模板是一个目录，入口为 `template.json`：

```json
{
  "name": "my-template",
  "schemaVersion": 1,
  "renderEngine": "node_bridge",
  "legacyTemplateDir": "packages/templates/my-template",
  "ownership": "third_party_extension",
  "useCases": ["create"],
  "migrationLayer": "custom"
}
```

| 字段                | 类型                                                                   | 说明               |
| ------------------- | ---------------------------------------------------------------------- | ------------------ |
| `name`              | string                                                                 | 模板 id            |
| `schemaVersion`     | number                                                                 | 清单版本           |
| `renderEngine`      | `rust_declarative` / `node_bridge`                                     | 渲染来源声明       |
| `legacyTemplateDir` | string                                                                 | 历史目录，用于兼容 |
| `ownership`         | `rust_preferred_node_fallback` / `node_only` / `third_party_extension` | 所有权             |
| `useCases`          | `create` / `add`                                                       | 适用场景           |
| `migrationLayer`    | string?                                                                | 迁移层标记         |

模板目录还可以提供四个可选模块：

| 文件                 | 导出 | 作用                                    |
| -------------------- | ---- | --------------------------------------- |
| `questions.js/ts`    | 函数 | 返回补问列表                            |
| `dependencies.js/ts` | 函数 | 返回 `dependencies` / `devDependencies` |
| `config.js/ts`       | 函数 | 返回 `files` 与 `outputTasks`           |
| `files/**/*.ejs`     | EJS  | 默认文件模板                            |

`config` 返回：

```ts
{
  outputTasks: ['write-files', 'install-deps'],
  files: [
    { outputPath: 'src/main.ts', templatePath: 'ejs/main.ts.ejs' }
  ]
}
```

没有 `config` 时，运行时会扫描 `files/**/*.ejs` 并按相对路径输出。

## 模板发现顺序

模板根目录按以下顺序合并：

1. 内置 `@lania-cli/templates` 的模板目录
2. `product.templatesDir`
3. `templateRuntimeDirs`
4. 旧名 `templateDirs`

后三个字段来自项目 `lan.config.*`，相对路径按项目根解析。当前实现会把所有 root 的模板都收集起来并按 `name` 排序；同名模板会同时出现，不会自动覆盖。

## 模板 Bridge 方法

### template.list

```json
{
  "templates": ["spa-react", "spa-vue", "toolkit"],
  "metadata": [
    {
      "name": "spa-vue",
      "title": "spa-vue",
      "description": null,
      "tags": [],
      "schemaVersion": 1,
      "renderEngine": "rust_declarative",
      "legacyTemplateDir": "packages/templates/spa-vue",
      "ownership": "rust_preferred_node_fallback",
      "useCases": ["create", "add"],
      "migrationLayer": "rust_builtin_template"
    }
  ]
}
```

schema manifest 中声明的 `templates[]` 会补充 `title` / `description` / `tags`。

### template.getQuestions

```json
{
  "template": "spa-vue",
  "questions": [
    {
      "message": "Please select a build tool:",
      "name": "buildTool",
      "choices": ["webpack", "vite"],
      "default": "vite",
      "type": "list"
    }
  ]
}
```

### template.getDependencies

```json
{
  "template": "spa-vue",
  "dependencies": ["vue"],
  "devDependencies": ["vite", "@vitejs/plugin-vue"]
}
```

### template.getOutputTasks

```json
{
  "template": "spa-vue",
  "tasks": ["write-files", "install-deps"]
}
```

### template.render

```json
{
  "template": "spa-vue",
  "schemaVersion": 1,
  "renderEngine": "rust_declarative",
  "files": [{ "path": "src/main.ts", "content": "..." }]
}
```

### addTemplate.render

```json
{
  "template": "v3",
  "schemaVersion": 1,
  "label": "vue3模板组件",
  "filename": null,
  "extname": "vue",
  "content": "<template>..."
}
```

## add 模板上下文

| 字段           | 默认        | 作用                               |
| -------------- | ----------- | ---------------------------------- |
| `projectName`  | `lania-app` | 组件/变量名                        |
| `language`     | `ts`        | `js` / `ts`，影响扩展名            |
| `cssProcessor` | `css`       | `css` / `sass` / `stylus` / `less` |

`addTemplate.render` 的 `filename` 为固定文件名；为 `null` 时由调用方使用 `projectName + extname` 组合输出路径。

## 下一步

- [模板与脚手架](/guide/scaffolding)
- [创建项目实战](/cookbook/create-project)
- [产品 CLI](/guide/product-cli)
