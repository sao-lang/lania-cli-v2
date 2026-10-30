---
'@lania-cli-v2/cli': minor
---

新增可独立分发的运行时管理器可执行文件 **`runex`**（nvm / fnm 那一类工具）：

- `runex install / use / remove / list / info / versions / which / init / doctor`，语义与 `lan env` 共用同一套 Store 与 Provider；区别是默认直接执行（`--dry-run` 才打印计划），并支持 `--json` 给脚本与 GUI 用。
- `runex install <tool>`（省略版本或写 `latest`）自动取上游最新稳定版；`runex use <tool>` 省略版本时读 `.nvmrc` / `.go-version` 等项目版本文件，`--project` 把解析后的具体版本写回。
- 只依赖 `runex-core` 与 `clap`，可用 `cargo build --release -p runex` 产出单文件；新增 `.github/workflows/release-runex.yml`，按 `runex-v*` tag 构建 linux/macOS/Windows 五个平台的 tarball / zip。
- shim 生成改为「按管理命令生成」：宿主内嵌继续写 `lan env which`，独立二进制写 `runex which` 并在单 token 命令下附带绝对路径兜底，避免没装 `lan` 的机器上 shim 失效。
- `runex-core` 删除从未使用的 `lania-exec` 依赖，对其它 lania crate 零依赖。
