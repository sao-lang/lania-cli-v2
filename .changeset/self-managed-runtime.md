---
'@lania-cli-v2/cli': major
---

`lan env` 改为 **lania 自管运行时**：`install` / `use` / `remove` 不再转发给 nvm / fnm / volta / pyenv / rustup，而是直接下载上游官方产物并维护 `~/.runex`。

- 第一批自管语言：Node、Python、Go、Rust、Deno、Bun、Zig；其余语言 `install` 会明确报错「尚未自管」。
- 新增 `lan env init`（打印把 `~/.runex/shims` 加入 PATH 的片段）、`lan env which`（shim 依赖的入口解析）、`lan env list --installed`、`lan env versions --available`。
- `install` 下载后强制校验官方 sha256；上游未提供校验和时做归档完整性校验，避免 CDN 截断产生半截产物。
- `remove` 删除 current 版本后会自动回退到剩余最高版本。
- 新增环境变量：`RUNEX_HOME`（覆盖 `~/.runex`）、`RUNEX_OFFLINE`、`RUNEX_HTTP_TIMEOUT_SECS`、`RUNEX_DOWNLOAD_TIMEOUT_SECS`、`GITHUB_TOKEN`/`GH_TOKEN`（提升 GitHub API 配额）。

**破坏性变更**：`lan env` 的 `--manager` / `-m` 选项已移除；`lan env versions` 只列 lania 自己管理的版本；`lan env install node 20` 不再调用 nvm/volta，而由 lania 下载安装。已由系统或其他管理器安装的运行时仍会在 `lan env list` 中显示（`source: system`），但不会被接管。
