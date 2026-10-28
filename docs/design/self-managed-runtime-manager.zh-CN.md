# 自管版本管理器技术方案

> 状态：**第一批已落地，包边界已完成收口**（Node / Python / Go / Rust / Deno / Bun / Zig）。
> 目标读者：能直接据此开工的 builder。
> 决策来源：需求方明确要求「lania 自己做版本管理器，不再转发给 nvm/volta 等」，
> 范围为 **只管理运行时环境**（安装/切换/卸载/列举），**不做全局包管理**，
> 覆盖代码中全部 40 种语言，并 **replace** 现有的探测/转发层。
>
> 落地情况见文末 §10「第一批实现记录」；**包边界与拆包已完成，见 §11**。
> 配套文档：`agent-architecture.zh-CN.md`（agent 拆包）、
> `multi-runtime-bridge.zh-CN.md`（多运行时 bridge 与 dev/build/create/add/lint 重构）。

## 1. 意图与动机

### 现状

`lan env` 目前是**编排层**，不做版本管理：

```text
lan env install node 20
  → plan.rs 选择管理器（nvm → fnm → volta → ...）
  → 拼出 `volta install node@20`
  → exec 执行
```

它自己不下载、不解压、不维护当前版本指针。
结果是：

- 行为完全取决于用户装了什么管理器，**不可预期**
- 36 种语言 × 44 个管理器的探测矩阵维护成本极高，且永远覆盖不全
- 用户必须先自己装好 nvm/pyenv/rustup 才能用 `lan env`

### 目标

lania 自己成为运行时管理器，提供**单一、可预期**的语义：

```text
lan env install node 20     → lania 自己从 nodejs.org 下载、解压、登记
lan env use node 20         → lania 切换 current 指针
lan env list --installed    → lania 读自己的清单
lan env remove node 18      → lania 删除自己的目录
```

### 非目标（明确不做）

- **不做全局包管理**：不提供 `lan env pkg`，不接管 `npm i -g` / `pip install` / `cargo install`。
  全局包仍由各语言工具自身管理，装在版本目录内，行为与原生一致。
- 不实现运行时增量更新/差分下载
- 不实现源码编译（全部使用官方预编译产物）
- 不接管系统级包管理器（apt/brew/winget）安装的运行时
- 不做跨机器同步/云端清单

## 2. 核心架构

```text
┌─────────────────────────────────────────────────────┐
│ CLI 层  lan env install/use/list/remove/doctor      │
├─────────────────────────────────────────────────────┤
│ Service 层                                          │
│  ├─ CatalogService   语言/版本清单（官方 release API）│
│  ├─ DownloadService  下载 + 校验和 + 解压            │
│  ├─ StoreService     版本目录布局 + current 指针     │
│  └─ ShimsService     ~/.runex/shims 生成与 PATH 注入 │
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
~/.runex/                        # RUNEX_HOME，可用环境变量覆盖
├── runtimes/
│   └── node/
│       ├── 20.11.1/
│       │   ├── bin/node
│       │   ├── bin/npm -> ../lib/node_modules/npm/bin/npm-cli.js
│       │   └── lib/node_modules/          # 原生 npm 全局包目录（lania 不接管）
│       ├── 22.21.1/
│       └── .runex-meta.json               # { "current": "20.11.1", "installed": [...] }
├── shims/                                 # 注入 PATH 的目录
│   ├── node
│   ├── npm
│   └── pnpm
├── cache/
│   └── downloads/                         # 产物缓存，避免重复下载
└── state.json                             # 全局状态（默认版本、项目映射）
```

> 项目级 `.lania/`（release-state、agent sessions、schemas）保持不变，两者职责不同（见 §9.2）。

### 3.1 current 指针

**不再依赖 PATH 顺序**。`current` 是 `.runex-meta.json` 里的一个字段：

```json
{
  "current": "20.11.1",
  "installed": ["18.20.8", "20.11.1", "22.21.1"]
}
```

`use` 只改这个字段 + 重新生成 shims，**不修改用户 PATH**。

### 3.2 shim 机制

`lan env init` 只做一件事：把 `~/.runex/shims` 放到 PATH 最前面。

shim 是一个很小的转发器（Windows 用 `.cmd` + `.exe` 方案，POSIX 用 shell 脚本）：

```bash
#!/usr/bin/env bash
exec "$(lan env which node --resolve)" "$@"
```

Windows 上为了避免 `lan` 自身依赖 node（鸡生蛋问题），shim 用 **Rust 编译的极小 launcher**，
直接读 `state.json` 定位真实二进制后 `exec`。

**这是替代现有 `volta run` shim 方案的关键改动** —— 现在的 `lan.cmd` 是 28 字节的
`volta run %~n0 %*`，一旦 Volta 不在就完全失效。

## 4. 全局包：明确不做

这一版**不提供** `lan env pkg`，也不接管任何语言的全局包。

### 4.1 理由

- 各语言包模型差异极大（npm 的 `--prefix`、pip 的 `--user`、cargo 的 `CARGO_HOME`、
  go 的 `GOBIN`），统一抽象会引入大量边界情况
- 全局包与运行时版本的兼容性无法由 lania 保证（如 Python 3.11 → 3.12 的
  site-packages 不兼容），强行管理会制造「切版本后包神秘失效」的问题
- 用户已有的包管理器（npm / pip / cargo）本身工作良好，替换无收益

### 4.2 行为约定

- 全局包仍然装在**对应版本的目录内**（如 `runtimes/node/20.11.1/lib/node_modules`）
- `lan env use` 切换版本后，全局包随版本变化 —— **与原生 nvm 行为一致**
- lania 只负责让 `node` / `npm` 等入口指向当前版本，不介入包层
- `lan env doctor` 会提示某版本的全局包数量，仅作观察，不做管理

### 4.3 后续可选

如果将来需要，可以新增独立的 `lan pkg` 命令族，与本方案解耦。
本方案不为其预留接口，避免过度设计。

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
阶段 2  `lan env install/use/list/remove` 切到新实现
阶段 3  删除 ManagerKind 与转发逻辑
```

不设 `--legacy` 逃生舱（见 §9.3）：切到新实现后旧路径直接移除。

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

### 6.3 第三批（全部自管，逐条确认上游可行性）

决策已确认这批**必须完整实现自管下载**，不做「只导入」降级。
因此开工前必须逐条确认上游产物，下表是**开工前置清单**：

| 语言     | 上游来源                        | 主要障碍                       | 可行性               |
| -------- | ------------------------------- | ------------------------------ | -------------------- |
| Ruby     | ruby-lang.org / ruby-build      | Windows 无官方预编译           | 需评估               |
| PHP      | windows.php.net / php.net       | 各平台包的扩展差异大           | 需评估               |
| Perl     | strawberryperl (Win) / perl.org | 主要依赖系统                   | 需评估               |
| Lua      | lua.org 源码包                  | **只有源码，无预编译**         | 需自行编译           |
| Julia    | julialang.org                   | 官方 tarball，较好             | 可做                 |
| Haskell  | ghcup 元数据                    | 依赖 ghcup 的绑定包列表        | 可做                 |
| Erlang   | erlang.org                      | 官方有预编译                   | 可做                 |
| Elixir   | github releases                 | 依赖 Erlang                    | 可做                 |
| Crystal  | github releases                 | 官方有 tarball                 | 可做                 |
| Nim      | nim-lang.org                    | 官方只有源码/部分二进制        | 需评估               |
| OCaml    | ocaml.org                       | 官方有 opam 二进制分发         | 可做                 |
| Clojure  | clojure.org                     | 仅 jar + 启动脚本              | 需明确「运行时」定义 |
| Groovy   | apache 分发                     | 依赖 JVM                       | 需明确「运行时」定义 |
| Scala    | scala-lang.org                  | 依赖 JVM，sbt 另管             | 需明确「运行时」定义 |
| R        | cran.r-project.org              | 依赖 BLAS/LAPACK               | 需评估               |
| Fortran  | 无统一上游                      | 实质是 gfortran（属 GCC）      | 需评估               |
| C / C++  | MSVC / MinGW / GCC              | 无统一上游，平台差异极大       | 需评估               |
| C# / F#  | dotnet.microsoft.com            | 产物是安装器，非 tarball       | 需评估               |
| Swift    | swift.org                       | Windows 支持有限               | 可做                 |
| Solidity | github (solc)                   | 只有单文件编译器，无「运行时」 | 需明确「运行时」定义 |
| Gleam    | github releases                 | 官方有预编译                   | 可做                 |
| Haxe     | haxe.org                        | 官方有预编译                   | 可做                 |
| Elm      | github releases                 | 官方有预编译                   | 可做                 |
| MoonBit  | moonbit 官方渠道                | 分发渠道需确认                 | 需评估               |
| Cangjie  | 华为官方渠道                    | 渠道不稳定，可能需申请         | 需评估               |
| ArkTs    | 华为官方渠道                    | 渠道不稳定                     | 需评估               |

**结论**：26 种中有约 10 种能直接做，10 种需评估，6 种需要先澄清
「什么叫这个语言的运行时」（Clojure/Groovy/Scala/Solidity 是库或单文件工具，
C/C++、Fortran 实为编译器集合）。这一项是开工前必须拍板的最大不确定点。

## 7. 风险

| 风险                               | 影响 | 处理                                                    |
| ---------------------------------- | ---- | ------------------------------------------------------- |
| 36 种语言 × 多平台矩阵维护成本     | 高   | 分三批；第三批先只做导入                                |
| 上游 CDN 地址/格式变更             | 中   | Provider 内收敛；校验和失败时明确报错不清空已有版本     |
| Windows shim 需 Rust 二进制        | 中   | 复用现有 `lania-cli` 加 `lan-shim` 子命令，避免额外产物 |
| 磁盘占用（多语言多版本）           | 中   | `lan env gc` 清理未使用版本 + 下载缓存上限              |
| 破坏性变更（删除 44 个管理器支持） | 高   | changeset 标 major；无 `--legacy` 回退                  |

## 8. 验收标准

- [ ] `lan env install node 20` 在无 nvm/volta 的干净环境可成功
- [ ] `lan env use node 18` 后 `node -v` 立即变化（不需重开终端）
- [ ] `lan env remove node 18` 只删除 lania 自己管理的目录
- [ ] `lan env list --installed` 只读 lania 自己的清单
- [ ] 删除 `ManagerKind` 后全仓无残留引用
- [ ] 现有 `env` e2e 测试全部迁移并通过

## 9. 已确认决策

| #   | 决策                | 结论                                                                      |
| --- | ------------------- | ------------------------------------------------------------------------- |
| 1   | 第三批语言范围      | **完整版**：全部 36 种语言都要自管下载安装，不做「只导入」降级            |
| 2   | `LANIA_HOME` 目录名 | **改名**，避免与项目级 `.lania/`（release-state 等）混淆 → 用 `~/.runex/` |
| 3   | `--manager` 逃生舱  | **不保留**。`lan env` 只有一条自管路径，不转发给外部管理器                |
| 4   | 能力层包名          | **改名** `lania-env` → `lania-runtime-manager`（见 §11）                  |

### 9.1 关于决策 1 的影响

第三批 24 种语言必须全部自管，意味着每种语言都要确定：

- 上游产物地址（官方 CDN / GitHub Releases / 语言自己的分发渠道）
- 产物形态（tar.gz / zip / tar.zst / 单文件 / 安装器）
- 平台矩阵（部分语言只支持特定 OS/arch）
- 校验方式（官方 SHA256 / 签名文件）
- 目录布局与环境变量（`JAVA_HOME`、`GOROOT`、`GEM_HOME` 等）

**其中难度最高的几种**需要在立项时单独确认可行性与授权：

| 语言                      | 主要障碍                                                      |
| ------------------------- | ------------------------------------------------------------- |
| Cangjie / ArkTs / MoonBit | 上游分发渠道不稳定，部分需申请或仅内部渠道                    |
| Solidity                  | 官方只有 `solc` 单文件，无「运行时」概念                      |
| C/C++                     | 无统一上游，实际是 libc/编译器集合，需按平台选 MSVC/MinGW/GCC |
| C# / F#                   | 依赖 .NET SDK，产物是安装器而非 tarball                       |
| Groovy / Scala / Clojure  | 依赖 JVM，实质是「库 + 启动脚本」                             |
| R / Fortran               | 依赖系统科学计算库（BLAS/LAPACK）                             |

**这 6 类如果找不到可靠的官方预编译产物，自管下载在工程上无法实现**，只能回退到「检测系统已装」。
§6.3 会把每种语言的可行性逐条列出，作为开工前必须先确认的清单。

### 9.2 关于决策 2 的影响

```text
~/.runex/                     # 新增：LANIA 的运行时根目录
├── runtimes/<lang>/<version>/
├── shims/
├── cache/downloads/
└── state.json
```

项目级 `.lania/`（release-state、agent sessions 等）保持不变，两者职责完全不同：

| 目录        | 用途                                | 作用域     |
| ----------- | ----------------------------------- | ---------- |
| `.lania/`   | 项目状态（release、agent、schemas） | 每项目一份 |
| `~/.runex/` | 运行时环境                          | 全局一份   |

### 9.3 关于决策 3 的影响

- Provider 层不再有 `ExternalManager` 回退实现
- `lang env install` 只有 lania 自己的下载路径
- §5.2 的删除清单保持不变：44 个 `ManagerKind` 全部移除
- **风险**：某些语言（见 §9.1）如果确实无法自管，会变成「无法安装」，
  而不是「降级到外部管理器」。这一点需要在上表逐条确认后再开工。

> 全局包管理已明确移出本方案（见 §4），不再作为待决策项。

## 10. 第一批实现记录

### 10.1 已交付范围

| 语言   | 上游                              | 产物                        | 校验和来源                     |
| ------ | --------------------------------- | --------------------------- | ------------------------------ |
| Node   | nodejs.org/dist                   | `tar.gz` / `zip`            | `SHASUMS256.txt`               |
| Python | astral-sh/python-build-standalone | `*-install_only.tar.gz`     | GitHub asset `digest`          |
| Go     | go.dev/dl                         | `tar.gz` / `zip`            | 上游未提供（做结构完整性校验） |
| Rust   | static.rust-lang.org              | `rust-*` + `cargo-*` 组件包 | 上游未提供（做结构完整性校验） |
| Deno   | github.com/denoland/deno/releases | `zip` 单文件                | GitHub asset `digest`          |
| Bun    | github.com/oven-sh/bun/releases   | `zip` 单文件                | GitHub asset `digest`          |
| Zig    | ziglang.org/download              | `tar.xz` 单目录             | `index.json` 的 `shasum`       |

模块划分（`rust/crates/lania-runtime-manager/src/`）：

```text
archive.rs   tar.gz / tar.xz / tar.zst / zip 解压 + 完整性校验
checksum.rs  sha256 计算、上游清单解析（GNU / BSD 两种风格）
detect.rs    只读探测（lania 自管版本优先，其次 PATH）
error.rs     分类错误（Platform / Network / Verify / Archive / Unexpected）
home.rs      ~/.runex 布局（RUNEX_HOME 覆盖）
http.rs      阻塞式 GET（重试 / 代理 / GitHub token / 截断检测）
platform.rs  OS / arch / libc 探测
provider/    每种语言一个 Provider（model.rs 定义契约）
shims.rs     ~/.runex/shims 生成（POSIX 脚本 + Windows .cmd）
state.rs     state.json 与每版本 .runex-meta.json
store.rs     安装 / 切换 / 卸载 / 列举 / which / doctor
version.rs   版本解析、前缀匹配、稳定版优先
plan.rs      dry-run 计划与执行（不再有 ManagerKind）
```

命令面（`lan env`）：

| 子命令                 | 作用                                                  |
| ---------------------- | ----------------------------------------------------- |
| `install <tool> <ver>` | 下载 + 校验 + 解压 + 登记 + 生成 shim                 |
| `use <tool> <ver>`     | 改 current 指针 + 重写 shim（`--project` 写版本文件） |
| `remove <tool> <ver>`  | 删除自管目录；删当前版本时回退到剩余最高版本          |
| `list [tool]`          | 列出工具与探测版本；`--installed` 只读自管清单        |
| `info <tool>`          | 单个工具详情 + `provider` 能力                        |
| `versions <tool>`      | 已安装版本；`--available` 额外查上游                  |
| `which <name>`         | 输出入口对应的真实路径（shim 依赖它）                 |
| `init [--shell]`       | 打印把 `~/.runex/shims` 加入 PATH 的片段              |
| `doctor`               | 自管状态 / 缺失 / 外部安装 / shim 与 PATH 诊断        |
| `ui`                   | 终端 TUI                                              |

### 10.2 与方案原文的偏差（逐条说明）

| #   | 方案原文                                                 | 实际实现                                                                     | 原因                                                                           |
| --- | -------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| 1   | Windows shim 用 Rust 编译的极小 launcher                 | 小程序用 `.cmd` 直接指向版本目录内的绝对路径；同时保留 POSIX 脚本给 Git Bash | 少一个产物、`node -v` 少一次进程启动；`use` 时重写绝对路径即可，无需额外二进制 |
| 2   | POSIX shim `exec "$(lan env which node --resolve)" "$@"` | 保留原方案                                                                   | 切换版本后立即生效                                                             |
| 3   | Python 用 `tar.zst` + strip                              | 改用 `install_only.tar.gz`（同源同内容，只是免去 `zstd` 强依赖的下载路径）   | `install_only.tar.gz` 已是扁平 `python/` 布局，无需 strip                      |
| 4   | Rust 采用 `rustup` 元数据                                | 直接读 `channel-rust-stable.toml`，下载 `rust` + `cargo` 两个组件包并合并    | 不引入 rustup；组件包合并后即为可用的 `bin/`、`lib/`、`share/`                 |
| 5   | `detect.rs` 保留为「导入外部运行时」                     | 保留为**只读探测**（`source: system`），不提供导入命令                       | 导入涉及复制系统目录，风险与收益不成正比，留待后续评估                         |
| 6   | 目录名 `RUNEX_HOME`                                      | 环境变量为 `RUNEX_HOME`，目录为 `~/.runex`                                   | 与决策 2 一致                                                                  |

### 10.3 第一批暴露的问题与处理

| 问题                                              | 现象                                       | 处理                                                                                                                                  |
| ------------------------------------------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `ureq` 全局超时把大文件下载判成超时               | `timeout: global`，30 秒必然失败           | 改为只限制 connect / 单次读取；下载总时限单独用 `RUNEX_DOWNLOAD_TIMEOUT_SECS`                                                         |
| 响应提前结束导致**静默截断**                      | 420MB 的 Rust 包只写 170MB，解压才报错     | 下载后比对 `Content-Length` 并校验归档结构；不一致立即删除缓存文件                                                                    |
| GitHub API `per_page=100` 对超大仓库直接 504      | python-build-standalone 永远拿不到版本清单 | 改为 `per_page=20` 分页，首页失败即报错、后续页失败用已有结果                                                                         |
| GitHub API 匿名配额 60 次/小时                    | 反复调试后 403                             | 支持 `GITHUB_TOKEN` / `GH_TOKEN`（5000 次/小时），403 错误信息直接提示怎么解                                                          |
| `channel-rust-stable.toml` 没有根层 `version`     | 拿到 `0.100.0`（cargo 版本）而不是 1.99.1  | 改为按 `[pkg.rust]` 段解析；`pkg.cargo` 只作兜底                                                                                      |
| python-build-standalone 每个 release 含多个次版本 | 只取第一个 asset，`3.12` 永远匹配不到      | 遍历全部 asset，按 `X.Y.Z` 去重（新 → 旧取第一个）                                                                                    |
| `lan env install go 1.22` 装到 `1.22rc2`          | 前缀匹配先命中预发布                       | `pick_best` 优先稳定版，只有无稳定版时才用预发布                                                                                      |
| 删除 current 版本后未回退                         | current 指针悬空、shim 指向已删除目录      | 先清指针再删目录，然后 `use` 剩余最高版本；无剩余则删 shim                                                                            |
| Rust 组件包解压后是 `rust-*/` + `cargo-*/` 嵌套   | `bin/` 不在根上，shim 找不到入口           | `materialize` 识别「全部带 `bin/` 的组件目录」并逐层合并                                                                              |
| `timeout_recv_body` 是**整体**预算而非单次读      | 慢网（~170KB/s）装 node 22 必然 30s 超时   | 拆出独立下载 agent：`timeout_per_read` 防卡死、`timeout_recv_body` 用 `RUNEX_DOWNLOAD_TIMEOUT_SECS`（2026-10 拆包时发现，第一批遗留） |

### 10.4 尚未覆盖（第二批 / 第三批）

- Java / Kotlin / Scala / Dart / Swift / D / V / Julia / Haskell 等：`provider::is_planned` 已列出，`install` 会明确提示尚未自管。
- 第三批（Ruby / PHP / Lua / C / C++ / C# 等）仍需按 §6.3 逐条确认上游可行性。
- `lan env gc`（清理未使用版本与下载缓存）尚未实现；当前缓存只增不减。

## 11. 包边界：拆成「能力层 + 命令层」

### 11.1 现状盘点

与 agent 不同，env 的**能力层已经天然独立**：

| crate                       | 行数 | 依赖 `lania-host` | 依赖 `lania-node-bridge` | 内容                      |
| --------------------------- | ---- | ----------------- | ------------------------ | ------------------------- |
| `lania-runtime-manager`     | 6123 | ❌                | ❌                       | 探测 / 下载 / 安装 / shim |
| `lania-plugins-command-env` | 3476 | ✅                | ❌                       | 命令 spec + TUI + 渲染    |

`lania-runtime-manager` 对**其它 lania crate 零依赖**：`lania-exec` 的声明从未被使用，
本轮已删除，现在只依赖第三方 crate（`ureq` / `tar` / `zip` / `xz2` / `zstd` / `sha2` / `serde`）。
`lania-plugins-command-env` 的 `PluginMeta.requires` 只有 `Exec + Fs`，**从不触碰 bridge**。

这说明 env 侧要做的不是「拆」，而是「**归位 + 冻结对外面**」：

- 「独立包」的判定标准不是「有没有单独目录」，而是**能不能脱离本仓单独编译/分发**。
  零 lania 依赖 + 冻引导出面，才让后续把它打包成独立可执行工具（`runex`，见 §12）成立。
- 命令层（`lan env` 的 spec / TUI / 渲染）继续留在 `lania-plugins-command-env`，
  不反向污染能力层。

### 11.2 改名：`lania-env` → `lania-runtime-manager`（已落地）

理由：`lania-env` 与 `lan env` 命令同名，但两者层级完全不同（一个是能力库，一个是命令实现）。
外部消费者读 `Cargo.toml` 时无法从名字判断该依赖哪个。改名后语义明确。

**实际影响面（全仓穷举，共 6 处）**：

| 文件                                                | 改动                                                                              |
| --------------------------------------------------- | --------------------------------------------------------------------------------- |
| `rust/Cargo.toml`                                   | `members` 里 `crates/lania-env` → `crates/lania-runtime-manager`                  |
| `rust/crates/lania-runtime-manager/Cargo.toml`      | `name = "lania-runtime-manager"`                                                  |
| `rust/crates/lania-plugins-command-env/Cargo.toml`  | 依赖键 + 路径改名                                                                 |
| `rust/crates/lania-cli/Cargo.toml`                  | 依赖键 + 路径改名（方案原文漏列，cli 入口直接用 `RuntimeHome`）                   |
| `rust/crates/lania-cli/src/main.rs`                 | 1 处 `lania_env::RuntimeHome` → `lania_runtime_manager::`                         |
| `rust/crates/lania-plugins-command-env/src/**/*.rs` | 6 个文件里全部 `lania_env` → `lania_runtime_manager`（不止 lib.rs 的 5 处 `use`） |

目录用 `git mv` 改名，历史保持连续；`rust/Cargo.lock` 由 cargo 重新生成。
另有三处注释提到旧名（`lania-config` / `lania-host` / `lania-runtime`），同步改为新名。

无外部消费者（全仓 crate 之间都是 path 依赖，均未发布到 crates.io；仓库里也没有任何
`publish = false` 声明，这一点与方案原文不符，已更正），故为**零风险改动**；
因为不涉及对外行为，没有单独新增 changeset（包名是仓库内部 crate 名，不是 npm 包）。

改名同时删掉了从未被使用的 `lania-exec` 依赖，使该 crate 对其它 lania crate **零依赖** ——
这是 §11.1 定下的「独立包」判定标准，也是 §12 把它做成独立可执行工具的前置条件。

### 11.3 对外稳定面（冻结清单，已落地）

改名后下列导出是**承诺稳定**的（供 GUI / IDE 插件 / CI 脚本嵌入）：

```rust
// 模型
pub use language::Language;
pub use platform::{Arch, Libc, Os, Platform};
pub use version::{pick_best, VersionTag};
pub use home::{RuntimeHome, HOME_ENV, META_FILE};

// 服务
pub use detect::{EnvService, InstalledVersion, ProjectVersion, ToolStatus};
pub use store::{RuntimeStore, InstallOutcome, InstallEvent, Diagnostic};

// 扩展点
pub use provider::{RuntimeProvider, ProviderContext, ProviderArtifact, provider_for};

// 计划与执行
pub use plan::{build_plan, execute_plan, EnvPlan, EnvAction, PlanStep};
```

**不承诺稳定**（内部实现，可自由重构）：

- `archive` / `checksum` / `http` / `shims` 的具体函数签名
- `store.rs`（47KB）内部的存储布局细节

**落地方式（已执行）**：`lib.rs` 把导出拆成两段带注释的分区 —— 「对外稳定面（冻结清单）」
与「内部实现（不承诺稳定）」，任何新导出都必须显式归到其中一段。冻结清单还由
`tests::frozen_export_surface_is_intact` 做**编译期守卫**：删掉/改名任何一项稳定导出都会让测试编译失败，
不会等到 GUI / IDE 插件那边才暴露。

### 11.4 新增：为多运行时 bridge 提供「工具链血缘」（已落地）

env 与其它命令之间当前唯一的接缝是 `~/.runex/shims` 目录，但**没有任何命令主动使用它**。
`lan env use node 22` 只改 `state.json` + 重写 shim 文件，而 `build` / `lint` / `create` 全部
直接继承父进程 PATH —— 于是 managed 版本不生效。

在 `lania-runtime-manager` 增加两个函数（详见 `multi-runtime-bridge.zh-CN.md` §6）：

```rust
impl RuntimeHome {
    /// 把 shims 目录前置到给定 PATH 值上（纯函数，可测）。
    pub fn prepend_shims_to_path(&self, current: Option<&str>) -> String;

    /// 让当前进程及其后续子进程优先使用自管运行时（幂等）。
    pub fn activate(&self);
}
```

**约束（必须写入代码注释，否则后续重构会踩雷）**：

> `RuntimeStore::which` 只读 `state.json`，**不得回退到 PATH 扫描**。
> POSIX shim 的实现是 `exec "$(lan env which <name> --resolve)" "$@"`，
> 若 `which` 也走 PATH，就会命中 shim 自身形成自引用死循环。

### 11.5 `lan env doctor` 的连带调整（已落地）

现状 `doctor` 会诊断「shims 是否在 PATH 中」。一旦 `lan` 每次都自动 `activate()`，
该诊断将永远通过，失去意义。改为两条独立诊断：

| 诊断项                 | 含义                                            | 修复建议                                   |
| ---------------------- | ----------------------------------------------- | ------------------------------------------ |
| 本进程 PATH 是否前置   | `lan` 自己是否成功注入（内部健康度）            | 报 bug；用户无需操作                       |
| shell profile 是否固化 | 用户 `.bashrc` / `$PROFILE` 是否写了 shims 路径 | 提示 `lan env init` 片段（外部终端才需要） |

实现位置：`store.rs::diagnose`（两条独立 `Diagnostic`）+ `shell_profile_has_shims`
（真读 profile 文件，避免把 `lan` 自己注入的 PATH 误判成用户已固化）。

### 11.6 验收标准（全部通过）

- [x] `cargo tree -p lania-runtime-manager` 中不出现 `lania-host` / `lania-node-bridge`。
- [x] 改名后 `cargo test --workspace` 全绿，`lan env` 全部 e2e 通过。
- [x] `prepend_shims_to_path` / `activate` 有单测覆盖幂等性与前置顺序
      （`home.rs`：前置顺序 / 幂等 / 陈旧条目剔除 / 空 PATH / shims 缺失时跳过）。
- [x] `lan env use node 22` 后，同一 shell 内 `lan build` 使用 22
      （`main.rs::activate_managed_runtimes` 在命令解析前 `activate()`，子进程继承 PATH；
      bridge 侧落地见 `multi-runtime-bridge.zh-CN.md` §6 / B0）。
- [x] 冻结导出面有编译期守卫：`tests::frozen_export_surface_is_intact`。

## 12. 独立可执行工具 `runex`（产品化拆包）

### 12.1 目标

§11 把能力层做成了「对其它 lania crate 零依赖」的独立包，但它的唯一入口仍是 `lan env`
子命令 —— 用户必须先装 lania CLI 才能管理运行时。本节的产出物是一个**独立的单文件可执行工具**：

```text
runex install node 22      # 下载 + 校验 + 解压 + 生成 shim
runex use node 22          # 切换 current 指针
node -v                    # 走 ~/.runex/shims，立即生效
```

定位与 nvm / fnm 同类：只管运行时版本（安装/切换/卸载/列举），不做全局包管理、
不依赖 Node 或 lania 宿主。

### 12.2 包结构

```text
rust/crates/runex/                 # bin crate（新增）
├── src/cli.rs                     # clap 命令定义（nvm 肌肉记忆 + <tool> 参数）
├── src/commands.rs                # 命令实现：只做参数校验与输出形态
├── src/main.rs                    # 入口 + 退出码
├── tests/cli.rs                   # 进程级冒烟测试（不联网）
└── README.md
```

依赖方向（单向，无环）：

```text
runex ──▶ lania-runtime-manager ──▶ 第三方 crate
lania-cli ──▶ lania-plugins-command-env ──▶ lania-runtime-manager
```

`runex` **不依赖** `lania-host` / `lania-command` / `lania-plugins-command-env`，
因此 `cargo build --release -p runex` 的产物可以单独分发。

### 12.3 命令面

| 命令                                  | 说明                                                                 |
| ------------------------------------- | -------------------------------------------------------------------- |
| `runex install <tool> [ver]`          | 安装；前缀（`20`）装 20.x 最高补丁，省略或 `latest` 取上游最新稳定版 |
| `runex use <tool> [ver] [--project]`  | 切换 current；省略版本时读 `.nvmrc` 等版本文件，`--project` 写回     |
| `runex remove <tool> <ver>`           | 删除自管版本；删当前版本会回退到剩余最高版本                         |
| `runex list [--installed]`            | 列出工具 / 只列自管版本                                              |
| `runex info <tool>`                   | 单个工具详情（版本、来源、上游、入口）                               |
| `runex versions <tool> [--available]` | 已安装版本；`--available` 额外查上游                                 |
| `runex which <name>`                  | 入口对应的真实路径（shim 依赖它）                                    |
| `runex init [--shell bash]`           | 打印把 shims 加入 PATH 的片段                                        |
| `runex doctor`                        | 指针 / shim / PATH 体检                                              |

全局开关：`--json`（stdout 只输出机器可读内容，进度走 stderr）、`--dry-run`（只打印计划）、
`--home <dir>`（覆盖 `RUNEX_HOME`，同样用于测试隔离）。

**与 `lan env` 的差异**（刻意的）：`lan env` 默认 dry-run、要 `--yes` 才执行（宿主内嵌、面向交互）；
`runex` 默认直接执行、`--dry-run` 才预览（命令行工具习惯）。

### 12.4 shim 必须跟着「管理命令」走

这是拆包时暴露的真实缺陷：原 POSIX shim 写死 `exec "$(lan env which <name> --resolve)"`，
独立安装的 `runex` 在没有 `lan` 的机器上会让所有 shim 失效。

修复：新增 `ShimHost { command, exe }`（`shims.rs`），由 Store 持有：

- 宿主内嵌默认 `ShimHost::lan()` → 文本与之前完全一致（`lan env which ...`）；
- 独立二进制 `ShimHost::binary("runex", current_exe())` → 生成：

  ```sh
  #!/bin/sh
  manager="$(command -v runex 2>/dev/null)"
  [ -n "$manager" ] || manager="/abs/path/to/runex"
  exec "$("$manager" which node --resolve)" "$@"
  ```

  `command -v` 优先保证升级/换路径后仍能工作，绝对路径是 PATH 上没有 `runex` 时的兜底。

- Windows `.cmd` 不变：它本来就写版本目录的绝对路径，不经过管理命令。

同一机制也把用户可见提示里的 `lan env` 换成了 `runex`（`which` 报错、`doctor` 建议）。

### 12.5 分发

| 场景       | 做法                                                                                                                                                                   |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 本地/单机  | `cargo build --release -p runex` → 单文件                                                                                                                              |
| 正式发布   | `.github/workflows/release-runex.yml`：`runex-v*` tag 触发，五平台矩阵（Linux x64/arm64、macOS x64/arm64、Windows x64），产出 `runex-<ver>-<platform>.tar.gz` / `.zip` |
| 版本节奏   | 与 npm 包解耦；`runex` 用自己的 tag 前缀，不跟 `@lania-cli-v2/cli` 一起发                                                                                              |
| 运行期依赖 | Windows 用 `-C target-feature=+crt-static` 构建，产物不依赖 `VCRUNTIME140.dll` / `ucrtbase.dll`（实测依赖只剩 kernel32/ws2_32 等系统库）；其他平台只需系统 libc        |

### 12.6 验收

- [x] `cargo build --release -p runex` 产出单文件，`cargo tree -p runex` 不含 lania 宿主 crate。
- [x] `runex` 进程级测试 11 条：help / list --installed / init / which 失败态 / install --dry-run 不落盘 /
      doctor JSON / 未知工具拒绝 / `--home` 覆盖 `RUNEX_HOME` / `install` 默认 latest /
      `use` 读项目版本文件 / `use` 无版本无文件时给出可操作报错。
- [x] `lania-runtime-manager` 单测 72 条全绿，含新增 `standalone_host_gets_its_own_command_and_fallback_path`。
- [x] 真实安装冒烟：`runex install node 22` → `use` → `which` → 通过 shim 执行 `node -v`（见 §12.7）。
- [x] 干净环境验证：`--locked` 发布构建、PATH 只留 system32（无 lan / cargo）、`RUNEX_HOME` 含空格路径下
      完成 install → use → shim 执行 → JSON 输出 → remove 全链路。
- [x] `--dry-run` 不产生任何目录/文件。
- [x] Windows 产物静态链接 CRT（`dumpbin /dependents` 无 `VCRUNTIME140.dll` / `ucrtbase.dll`）。

### 12.7 冒烟记录（本机 Windows x64，真实下载）

```text
$ runex --home <tmp> install node 22
resolving version...
downloading node-v22.23.4-win-x64.zip      # 35.6MB，慢网 ~170KB/s，耗时 1m50s
verifying checksum...                       # SHASUMS256.txt 命中
extracting...
laying out...
installed 22.23.4
installed node 22.23.4 -> <tmp>\runtimes\node\22.23.4

$ runex list --installed
* node       22.23.4        <tmp>\runtimes\node\22.23.4

$ runex which node
<tmp>\runtimes\node\22.23.4\node.exe

$ cat <tmp>\shims\node.cmd
@echo off
rem Generated by runex. Serves `node` of the current runtime version.
"<tmp>\runtimes\node\22.23.4\node.exe" %*

$ PATH=<tmp>\shims:$PATH node -v
v22.23.4                                    # 走 shim，托管版本生效

$ runex doctor
[info] <tmp>\shims is not in your shell profile; run `runex init` ...
```

这一轮同时证明了两件事：独立二进制能完成完整安装链路（下载 → 校验 → 解压 →
布局 → 登记 → shim），以及 §10.3 新增的下载超时修复在真实慢网下是必需的 ——
未修复前同一个命令在 30s/120s 必然失败。
