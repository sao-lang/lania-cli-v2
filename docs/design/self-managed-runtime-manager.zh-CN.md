# 自管版本管理器技术方案

> 状态：**待评审**。目标读者：能直接据此开工的 builder。
> 决策来源：需求方明确要求「lania 自己做版本管理器，不再转发给 nvm/volta 等」，
> 深度为 `managed-packages`（运行时 + 全局包一起管），覆盖代码中全部 36 种语言，
> 并 **replace** 现有的探测/转发层。

## 1. 意图与动机

### 现状

`lan env` 目前是**编排层**，不做版本管理：

```text
lan env install node 20
  → plan.rs 选择管理器（nvm → fnm → volta → ...）
  → 拼出 `volta install node@20`
  → exec 执行
```

它自己不下载、不解压、不维护当前版本指针，也不管全局包。
结果是：

- 行为完全取决于用户装了什么管理器，**不可预期**
- 全局包与版本管理割裂（`npm i -g` 装到版本目录，切版本即丢失）
- 无法回答「这个版本下装了哪些包」「换版本如何迁移包」这类问题
- 36 种语言 × 44 个管理器的探测矩阵维护成本极高，且永远覆盖不全

### 目标

lania 自己成为版本管理器，提供**单一、可预期**的语义：

```text
lan env install node 20     → lania 自己从 nodejs.org 下载、解压、登记
lan env use node 20         → lania 切换 current 指针
lan env list --installed    → lania 读自己的清单
lan env pkg add pnpm        → 全局包装在当前版本，切版本自动跟随
```

### 非目标

- 不实现语言运行时的增量更新/差分下载
- 不实现运行时源码编译（除 Rust 走 rustup 的二进制下载外，全部用官方预编译产物）
- 不接管系统级包管理器（apt/brew/winget）安装的运行时
- 不做跨机器同步/云端清单

## 2. 核心架构

```text
┌─────────────────────────────────────────────────────┐
│ CLI 层  lan env install/use/list/remove/pkg/doctor  │
├─────────────────────────────────────────────────────┤
│ Service 层                                          │
│  ├─ CatalogService   语言/版本清单（官方 release API）│
│  ├─ DownloadService  下载 + 校验和 + 解压            │
│  ├─ StoreService     版本目录布局 + current 指针     │
│  ├─ ShimsService     ~/.lania/shims 生成与 PATH 注入 │
│  └─ PackageService   全局包安装/列举/迁移           │
├─────────────────────────────────────────────────────┤
│ Provider 层（每种语言一个实现）                      │
│  NodeProvider / PythonProvider / RustProvider / ... │
└─────────────────────────────────────────────────────┘
```

### 2.1 为什么必须分 Provider 层

不同语言的产物形态差异极大，无法用一套逻辑硬编码：

| 形态             | 代表                              | 下载物                              | 解压后                     |
| ---------------- | --------------------------------- | ----------------------------------- | -------------------------- |
| tarball 单目录   | Node                              | `node-v20.x-linux-x64.tar.gz`       | `bin/`、`lib/`、`include/` |
| tarball 需 strip | Python（python-build-standalone） | `cpython-3.12.x+...tar.zst`         | 需定位 `python/install/`   |
| 单文件           | Deno、Bun                         | `deno-x86_64-unknown-linux-gnu.zip` | 单个可执行文件             |
| 安装器           | Go、Rust                          | `.tar.gz` / `.msi`                  | 固定前缀                   |
| 需额外 SDK       | Dart、Java、Kotlin                | zip / tar.gz                        | 需设置 `JAVA_HOME` 等      |
| 无官方预编译     | Cangjie、ArkTs、MoonBit           | 需按语言特殊处理                    | 可能只有 nightly           |

所以 Provider trait 必须能表达「下载什么 → 怎么解压 → 目录怎么摆 → 需要哪些环境变量」。

### 2.2 Provider trait 草案

```rust
pub trait RuntimeProvider: Send + Sync {
    fn id(&self) -> &'static str;

    /// 查询该语言全部可用版本（可带缓存）
    fn list_versions(&self, http: &HttpClient) -> Result<Vec<VersionInfo>>;

    /// 给定版本 + 平台，返回要下载的产物信息
    fn resolve_artifact(
        &self,
        version: &Version,
        platform: &Platform,
    ) -> Result<ArtifactPlan>;

    /// 解压并整理成统一的 store 布局，返回 bin 目录与需要的 env
    fn install_layout(&self, extracted: &Path, target: &Path) -> Result<Layout>;

    /// 生成 shim 时需要包住的入口（如 node、npm、npx、corepack）
    fn entrypoints(&self) -> Vec<&'static str>;
}

pub struct Layout {
    pub bin_dir: PathBuf,
    /// 该运行时需要的环境变量，例如 JAVA_HOME、GOROOT、CARGO_HOME
    pub env: Vec<(String, String)>,
}
```

## 3. 目录布局

```text
~/.lania/                        # LANIA_HOME，可用环境变量覆盖
├── runtimes/
│   └── node/
│       ├── 20.11.1/
│       │   ├── bin/node
│       │   ├── bin/npm -> ../lib/node_modules/npm/bin/npm-cli.js
│       │   └── lib/node_modules/          # ← 全局包安装在这里
│       ├── 22.21.1/
│       └── .lania-meta.json               # { "current": "20.11.1", "installed": [...] }
├── shims/                                 # 注入 PATH 的目录
│   ├── node
│   ├── npm
│   └── pnpm
├── cache/
│   └── downloads/                         # 产物缓存，避免重复下载
└── state.json                             # 全局状态（默认版本、项目映射）
```

### 3.1 current 指针

**不再依赖 PATH 顺序**。`current` 是 `.lania-meta.json` 里的一个字段：

```json
{
  "current": "20.11.1",
  "installed": ["18.20.8", "20.11.1", "22.21.1"]
}
```

`use` 只改这个字段 + 重新生成 shims，**不修改用户 PATH**。

### 3.2 shim 机制

`lan env init` 只做一件事：把 `~/.lania/shims` 放到 PATH 最前面。

shim 是一个很小的转发器（Windows 用 `.cmd` + `.exe` 方案，POSIX 用 shell 脚本）：

```bash
#!/usr/bin/env bash
exec "$(lan env which node --resolve)" "$@"
```

Windows 上为了避免 `lan` 自身依赖 node（鸡生蛋问题），shim 用 **Rust 编译的极小 launcher**，
直接读 `state.json` 定位真实二进制后 `exec`。

**这是替代现有 `volta run` shim 方案的关键改动** —— 现在的 `lan.cmd` 是 28 字节的
`volta run %~n0 %*`，一旦 Volta 不在就完全失效。

## 4. 全局包管理（核心差异化能力）

这是需求方最关心的点。设计要点：**包跟人走，不跟版本走**。

```text
~/.lania/packages/               # 全局包的实际安装位置（与运行时版本解耦）
├── node/
│   └── pnpm/20.10.0/
│       ├── package.json
│       └── bin/pnpm
└── python/
    └── ruff/0.6.0/
```

### 4.1 三种落地策略

不同语言的包模型差异太大，需要分级处理：

| 策略                  | 适用                                    | 做法                                                           |
| --------------------- | --------------------------------------- | -------------------------------------------------------------- |
| **`--prefix` 重定向** | Node（npm）、Python（pip）              | 安装到 `~/.lania/packages/<lang>/`，再把该目录的 bin 加进 shim |
| **工具原生隔离**      | Rust（cargo install）、Go（go install） | 设置 `CARGO_HOME` / `GOBIN` 指向 lania 目录                    |
| **不支持**            | 语言原生无全局包概念                    | 明确报 `unsupported`，不假装支持                               |

### 4.2 命令行

```bash
lan env pkg add pnpm@10        # 装全局包（当前语言版本）
lan env pkg list               # 列出全局包
lan env pkg remove pnpm        # 卸载
lan env pkg upgrade            # 全部升级
lan env pkg migrate --from 18 --to 20   # 版本迁移时重装
```

### 4.3 关键不变量

- 全局包**不受 `lan env use` 影响**（切版本后包还在，且 shim 继续可用）
- 包与运行时的兼容性由语言自身保证；lania 只负责在**换大版本**时给出警告
  （例如 Python 3.11 → 3.12 的 site-packages 不兼容）

## 5. 与现有代码的关系（replace 策略）

### 5.1 保留复用

| 现有模块                                              | 处置                                          |
| ----------------------------------------------------- | --------------------------------------------- |
| `language.rs`（36 语言枚举、version_files、parse_id） | **保留**，作为 Provider 的元数据来源          |
| `detect.rs`（探测本机已装运行时）                     | **保留**，改为「导入外部运行时」的可选能力    |
| `plan.rs`（dry-run 计划模型）                         | **保留语义**，但改为生成 lania 自己的操作计划 |
| `managers.rs`（44 管理器探测）                        | **删除**，被 Provider 完全替代                |

### 5.2 删除

- `ManagerKind` 全部 44 个变体
- `install_steps` / `use_steps` 里的管理器分派逻辑（约 300 行）
- `lan env` 的 `--manager` 选项
- 文档中「复用 nvm/pyenv/rustup」的叙述

### 5.3 迁移路径

```text
阶段 1  新增 Provider 层 + StoreService + ShimsService（不动现有命令）
阶段 2  `lan env install/use/list` 切到新实现，旧路径保留 `--legacy` 开关
阶段 3  `lan env pkg` 上线
阶段 4  删除 ManagerKind 与转发逻辑，移除 `--legacy`
```

## 6. Provider 实现矩阵

36 种语言按产物形态分组，工作量差异极大：

### 6.1 第一批（官方预编译、格式规整）

| 语言   | 上游                               | 形态                     | 预计工作量 |
| ------ | ---------------------------------- | ------------------------ | ---------- |
| Node   | nodejs.org/dist                    | tar.gz / zip，多平台矩阵 | 1 天       |
| Python | python-build-standalone (indygreg) | tar.zst，需 strip        | 1.5 天     |
| Go     | go.dev/dl                          | tar.gz，固定前缀         | 0.5 天     |
| Rust   | static.rust-lang.org               | tar.gz + rustup 元数据   | 1 天       |
| Deno   | github releases                    | zip 单文件               | 0.5 天     |
| Bun    | github releases                    | zip 单文件               | 0.5 天     |
| Zig    | ziglang.org                        | tar.xz                   | 0.5 天     |

### 6.2 第二批（需要额外环境变量或 SDK）

| 语言   | 额外要求                                  |
| ------ | ----------------------------------------- |
| Java   | 需设 `JAVA_HOME`；版本源可用 Adoptium API |
| Kotlin | 依赖 JDK，需 `KOTLIN_HOME`                |
| Scala  | 依赖 JDK，sbt 另管                        |
| Dart   | zip + `PUB_CACHE`                         |
| Swift  | 仅 macOS/Linux，需系统依赖                |

### 6.3 第三批（无官方预编译，需特殊处理）

Cangjie、ArkTs、MoonBit、Solidity、Fortran、Gleam、Haxe、Elm、Nim、Crystal、
OCaml、Clojure、Julia、Haskell、Erlang、Elixir、Lua、Perl、PHP、Ruby、C/C++、
C#/F#、Groovy、R

这些要么依赖系统包管理器，要么只有 nightly，要么产物形态各异。
**建议**：这批先做「探测 + 导入」（把系统已装的纳入 lania 视图），
自管下载留到后续迭代，避免为了对齐 36 种语言而无限拖延。

## 7. 风险

| 风险                               | 影响 | 处理                                                    |
| ---------------------------------- | ---- | ------------------------------------------------------- |
| 36 种语言 × 多平台矩阵维护成本     | 高   | 分三批；第三批先只做导入                                |
| 上游 CDN 地址/格式变更             | 中   | Provider 内收敛；校验和失败时明确报错不清空已有版本     |
| Windows shim 需 Rust 二进制        | 中   | 复用现有 `lania-cli` 加 `lan-shim` 子命令，避免额外产物 |
| 磁盘占用（多语言多版本）           | 中   | `lan env gc` 清理未使用版本 + 下载缓存上限              |
| 破坏性变更（删除 44 个管理器支持） | 高   | 阶段 2 提供 `--legacy`；changeset 标 major              |
| 全局包跨版本兼容                   | 中   | 只在换大版本时警告，不强改                              |

## 8. 验收标准

- [ ] `lan env install node 20` 在无 nvm/volta 的干净环境可成功
- [ ] `lan env use node 18` 后 `node -v` 立即变化（不需重开终端）
- [ ] `lan env pkg add pnpm` 后切版本，`pnpm` 仍可用
- [ ] `lan env list --installed` 只读 lania 自己的清单
- [ ] 删除 `ManagerKind` 后全仓无残留引用
- [ ] 现有 `env` e2e 测试全部迁移并通过

## 9. 待决策

1. **第三批语言**是否接受「先只做导入」？否则工期不可控。
2. `LANIA_HOME` 默认位置：`~/.lania` 还是跟随现有 `.lania/` 项目目录约定？
3. 是否保留 `--manager` 作为「用外部管理器装，但登记到 lania 清单」的逃生舱？
4. 全局包模型：`~/.lania/packages` 共享 vs 每版本独立？（本方案选了共享）
