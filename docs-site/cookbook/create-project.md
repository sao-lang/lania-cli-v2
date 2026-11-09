# 创建项目

<div class="lead">
目标：从空目录创建一个 Vue 项目，完成预览、生成、安装、开发、构建与 Lint。
</div>

## 步骤 1：确认环境

```bash
node -v
pnpm -v
cargo -V
```

## 步骤 2：查看模板

```bash
lan workspace create template
lan workspace create template spa-vue
```

## 步骤 3：预览

```bash
lan workspace create --name demo-app --template spa-vue --preview
```

检查输出中的：

- 文件列表
- 依赖列表
- 安装命令
- Git 初始化计划

## 步骤 4：创建

```bash
lan workspace create \
  --name demo-app \
  --template spa-vue \
  --package-manager pnpm
```

如果不需要安装依赖：

```bash
lan workspace create --name demo-app --template spa-vue --skip-install
```

`create` 不只面向前端项目：把 `--template` 换成目标运行时，安装阶段会自动选择对应包管理器命令（Go 用 `go mod tidy`、Python 用 `pip/uv/poetry`、Rust 用 `cargo fetch`、Dart 用 `dart pub get`）：

```bash
lan workspace create --name svc --template go-service --package-manager go
lan workspace create --name cli --template python-cli --package-manager uv
```

## 步骤 5：进入项目

```bash
cd demo-app
```

## 步骤 6：开发

```bash
lan workspace dev --port 3001
```

## 步骤 7：构建

```bash
lan workspace build --mode production
```

## 步骤 8：Lint

```bash
lan workspace lint
```

## 步骤 9：补充组件

```bash
lan workspace add --name button --template v3
lan tools view ./src/button.vue
```

## 预期结果

- `demo-app/` 目录生成。
- 依赖安装完成（未使用 `--skip-install`）。
- `lan workspace dev` 输出开发服务器 URL。
- `lan workspace build` 生成构建产物。
- `lan workspace add` 生成组件文件。

## 常见问题

### 依赖版本查询失败

确认 registry 可访问。离线时使用 `--skip-install`，或在模板中写死版本。

### 目标目录已存在

`create` 会做冲突检测。换一个项目名，或先清理目标目录。

## 下一步

- [模板与脚手架](/guide/scaffolding)
- [模板参考](/reference/templates)
- [CLI 命令参考](/reference/cli)
