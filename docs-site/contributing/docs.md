# 文档站维护

<div class="lead">
学习文档站使用 VitePress 构建，目录为 docs-site/。本页说明启动、构建、目录约定与新增页面步骤。
</div>

## 启动

```bash
pnpm docs:dev
```

## 构建

```bash
pnpm docs:build
```

## 预览

```bash
pnpm docs:preview
```

## 目录

```text
docs-site/
  .vitepress/
    config.ts
    theme/
      index.ts
      custom.css
  public/
  index.md
  guide/
  reference/
  architecture/
  cookbook/
  contributing/
```

## 新增页面

1. 在对应目录创建 `.md` 文件。
2. 在 `.vitepress/config.ts` 的 sidebar 中增加链接。
3. 确保页面标题与导航文案一致。
4. 运行 `pnpm docs:build` 检查死链与构建错误。

## 写作约定

- 每页以 `# 标题` 开始。
- 开头使用 `<div class="lead">` 概括本页目标。
- 命令示例使用 fenced code block。
- 选项优先用表格。
- 页面末尾提供“下一步”。
- 引用仓库文件时使用反引号路径。
- 不写未在当前实现中验证的命令。

## 主题

`custom.css` 覆盖：

- Vue 绿色品牌色
- 首页 Hero
- Feature 卡片悬停
- 表格与代码块圆角
- 标题分隔线

如需调整品牌色，优先修改变量：

```css
:root {
  --vp-c-brand-1: #42b883;
  --vp-c-brand-2: #3aa876;
  --vp-c-brand-3: #2f9165;
}
```

## 依赖安装

如果使用 pnpm：

```bash
pnpm add -Dw vitepress
```

如果工作区 store 不兼容，也可以在 `docs-site/` 内单独安装：

```bash
cd docs-site
npm install
```

如果遇到 Rollup 原生包缺失（npm 可选依赖问题）：

```bash
npm install --prefix docs-site --no-save --registry=https://registry.npmjs.org \
  @rollup/rollup-win32-x64-msvc@4.64.10
```

Linux / macOS 请换成对应的 `@rollup/rollup-<platform>-<arch>` 包。

## 下一步

- [本地开发](/contributing/development)
- [测试与验收](/contributing/testing)
