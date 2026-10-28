# runex

独立的多运行时版本管理器：`install` / `use` / `remove` / `list` / `which` / `init` / `doctor`，
不依赖 nvm、volta、pyenv、rustup 等外部管理器，也不需要 Node 运行时。

```bash
cargo build --release -p runex        # -> rust/target/release/runex
runex install node 22                 # 下载 + 校验 + 解压 + 生成 shim
runex use node 22                     # 切换 current 指针
node -v                               # 走 ~/.runex/shims，立即生效
```

## 目录

```text
$RUNEX_HOME/                # 默认 ~/.runex
├── runtimes/<tool>/<ver>/  # 每次安装一个目录，里面是该版本的完整发行包
├── shims/                  # 唯一需要进 PATH 的目录
├── cache/downloads/        # 产物缓存
└── state.json              # 每个工具的 current 指针
```

## 常用命令

| 命令                                  | 说明                                                                 |
| ------------------------------------- | -------------------------------------------------------------------- |
| `runex install <tool> [ver]`          | 安装；前缀（`20`）装 20.x 最高补丁，省略或 `latest` 取上游最新稳定版 |
| `runex use <tool> [ver] [--project]`  | 切换 current；省略版本时读 `.nvmrc` 等版本文件，`--project` 写回     |
| `runex remove <tool> <ver>`           | 删除自管版本；删当前版本会回退到剩余最高版本                         |
| `runex list [--installed]`            | 列出工具 / 只列自管版本                                              |
| `runex info <tool>`                   | 单个工具详情                                                         |
| `runex versions <tool> [--available]` | 已安装版本；`--available` 额外查上游                                 |
| `runex which <name>`                  | 入口对应的真实路径（shim 依赖它）                                    |
| `runex init [--shell bash]`           | 打印把 shims 加入 PATH 的片段                                        |
| `runex doctor`                        | 指针 / shim / PATH 体检                                              |

全局开关：`--json`（脚本与 GUI 用）、`--dry-run`（只打印计划）、`--home <dir>`（覆盖 `RUNEX_HOME`）。

## 为什么可以单文件分发

`runex` 只依赖 `lania-runtime-manager` 和 `clap`；`lania-runtime-manager` 对其它 lania crate
**零依赖**（见 `docs/design/self-managed-runtime-manager.zh-CN.md` §11/§12），
因此 `cargo build --release -p runex` 产出的就是可独立分发的可执行文件。
