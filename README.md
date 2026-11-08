<p align="center">
  <img src="./docs-site/public/logo.svg" width="96" height="96" alt="Lania CLI logo">
</p>

# Lania CLI v2

[English](./README.md) | [简体中文](./README.zh-CN.md)

`Lania CLI v2` is a dual-runtime CLI: Rust owns the host runtime, command graph, workflows, and unified output model; Node Bridge owns JavaScript ecosystem integration, config loading, templates, and runtime extensions.

## Documentation

- [Chinese README](./README.zh-CN.md)
- [VitePress Learning Docs](./docs-site/index.md) · [Online Docs](https://sao-lang.github.io/lania-cli-v2/)
- [Docs Index](./docs/README.zh-CN.md)
- [Learning Guide](./docs/guides/learning-map.zh-CN.md)
- [Runtime Architecture](./docs/architecture/runtime-architecture.zh-CN.md)
- [Framework And Product Overview](./docs/architecture/框架化与产品化总览.zh-CN.md)
- [Design Notes And Reconciliation](./docs/design/专题设计与对账.zh-CN.md)
- [Node Bridge Protocol](./docs/archive/phase0/node-bridge-protocol.md)

## Current Capabilities

- 11 built-in commands: `dev`, `build`, `lint`, `tools`, `create`, `add`, `release`, `git`, `env`, `pi`, `config` — template queries are a `create` subcommand (`lan workspace template`)
- Bridge-backed execution for compiler and lint workflows, plus mixed host/bridge tooling utilities under `tools`
- Rust workflows for multi-runtime scaffolding, release orchestration, and Git automation
- Project-level dynamic commands loaded from manifest files
- Project plugins with source constraints, allowlists, and method restrictions
- Config support for `lan.config.js`, `lan.config.cjs`, `lan.config.json`, `lan.config.ts`
- Output modes: `json`, `human`

## Architecture

Everything is split into self-contained packages under `packages/`; each ships a
`lania.package.json` (single source of truth whose `description` field states its role):

- `packages/commands/*` (6): `workspace` (the merged dev/build/lint/create/add namespace, also carrying the `product` tree, `template` and `config` subcommands), plus `release`, `git`, `tools`, `env`, `pi`; `config` is built into the main binary
- `packages/distribution/core`: the scheduler — the single `lan` entry that dispatches to per-command binaries via the static `known-commands.json`
- `packages/distribution/cli`: the umbrella package aggregating all command subpackages and the full bridge slices
- `packages/foundation/*`: shared building blocks — `bridge-runtime` (stdio core + slice assembly), `bridge-slices/*` (12 TS slices), `templates` (10 project templates + EJS), `cli-kit` (output rendering), `host-core` (host seams), `package-config` (per-package config engine), `command` (spec/parsing/localization), `capabilities/*` (16 leaf crates)
- `packages/products/*`: standalone products — `runex` (self-managed runtime manager), `bridge-hub` (language-neutral message bus with 5 language adaptors), `agent` (coding agent engine)
- `packages/runtime`: the runtime facility executing project-level dynamic commands (`lan-runtime`)

In practice:

- Rust decides what command exists and how it is executed; the launcher dispatches to installed subpackage binaries.
- Node loads JS-native capabilities through bridge slices assembled per package payload.
- Each subpackage is independently installable (`npm i @lania-cli-v2/cmd-<cmd>`).

## Command Categories

| Command Group                         | Path                                                     | Description                                                          |
| ------------------------------------- | -------------------------------------------------------- | -------------------------------------------------------------------- |
| `workspace dev` / `build` / `lint`    | Rust -> Node Bridge -> compiler/lint slices              | JS toolchain execution (dev/build/lint merged under `workspace`)     |
| `workspace create` / `add`            | Rust workflow + template bridge calls                    | multi-runtime project and partial scaffolding                        |
| `workspace template`                  | Rust query -> Node Bridge                                | template catalog and template detail lookup                          |
| `workspace product`                   | Rust workflow -> compiler/product slices                 | product dev/build/pack/publish/inspect/doctor                        |
| `workspace config`                    | main-override aware per-package config                   | workspace-level preferences (main override > package file > default) |
| `tools list` / `run` / `view`         | Rust -> Node Bridge / Rust host utilities                | enumerate commands, execute files, display local files               |
| `release`, `git`                      | Rust workflow                                            | release orchestration and Git automation                             |
| `env`, `pi`                           | Rust host / Bridge                                       | runtime management (18 toolchains, TUI) and the coding agent         |
| `config` (built into the main binary) | main package preferences + per-package override segments | global/project/override configuration management (isolated writes)   |
| dynamic commands                      | project manifest -> Rust registration -> Node execution  | runtime command extension                                            |

## Project templates

`lan create` ships 10 built-in templates across five runtimes (Node.js, Python, Go, Rust, Dart):

| Template           | Runtime | Stack / package manager                     |
| ------------------ | ------- | ------------------------------------------- |
| `spa-react`        | Node.js | React SPA (Vite); npm / pnpm / yarn / bun   |
| `spa-vue`          | Node.js | Vue 3 SPA (Vite); npm / pnpm / yarn / bun   |
| `toolkit`          | Node.js | TypeScript toolkit; npm / pnpm / yarn / bun |
| `toolkit-monorepo` | Node.js | pnpm monorepo; npm / pnpm / yarn / bun      |
| `nestjs`           | Node.js | NestJS server; npm / pnpm / yarn / bun      |
| `express`          | Node.js | Express server; npm / pnpm / yarn / bun     |
| `go-service`       | Go      | Go service / CLI; go modules                |
| `rust-lib`         | Rust    | Rust library; cargo                         |
| `python-cli`       | Python  | Python CLI; pip / uv / poetry               |
| `dart-cli`         | Dart    | Dart CLI; dart pub                          |

`--package-manager` accepts 11 values: `npm`, `pnpm`, `yarn`, `bun`, `pip`, `uv`, `poetry`, `go`, `cargo`, `dart`, `pub`.

Install commands are selected by runtime: Node -> `npm/pnpm/yarn/bun install`, Python -> `pip/uv/poetry`, Go -> `go mod tidy`, Rust -> `cargo fetch`, Dart -> `dart pub get`. Language templates default to `skipInstall: true`; Node framework templates install dependencies by default.

```bash
lan workspace create --name svc --template go-service --package-manager go
lan workspace create --name cli --template python-cli --package-manager uv
```

## Unified configuration (three layers)

Configuration is managed in three layers, all reachable from the `lan config` management plane:

| Layer               | Location                                   | Scope                                                                 |
| ------------------- | ------------------------------------------ | --------------------------------------------------------------------- |
| Project             | `lan.config.js` / `.cjs` / `.json` / `.ts` | build/lint adapters, plugins, dynamic commands, UI                    |
| Global preferences  | `~/.lania/preferences.json`                | `locale`, `outputMode`, `logTimestamps` for every project             |
| Package preferences | `~/.lania/config/<pkg>.json`               | one installed command subpackage, keys declared by its `configSchema` |

All 10 command subpackages (`add`, `build`, `create`, `dev`, `env`, `git`, `lint`, `pi`, `release`, `tools`) now ship a `config` subcommand — `bare` / `get` / `set` / `init` / `doctor` / `sync` — with bilingual (zh/en) messages; `get` / `set` accept `--global` to target the global scope. `config` itself is the management plane, so the CLI has 11 commands after `generate` was archived. Common keys are `locale` (`zh` / `en`), `output.mode` (`json` / `human`) and `log.timestamps` (bool); `env` additionally declares `shim.host` (`lan` / `runex`). The shared implementation is centralized in `packages/foundation/package-config/rust/lania-command-config` instead of being duplicated per package.

```bash
# read / write one subpackage (--package accepts short name, npm full name,
# bin name or crate name; all are normalized)
lan config get locale --package env
lan config set locale zh --package @lania-cli-v2/cmd-env
lan config set shim.host runex --package lan-env

# list declared keys, defaults and current values of all installed subpackages
lan config packages
lan config packages --json

# push global preferences down to every installed and enabled subpackage
lan config apply
lan config apply --package env --set locale=zh --set output.mode=human
```

`--package` values are normalized across `env`, `@lania-cli-v2/cmd-env`, `lan-env` and the crate name, and writes are validated against the target package's `configSchema`. `apply` only overwrites keys the subpackage has declared (dot-separated keys map to camelCase global keys). The static `known-commands.json` now carries each subpackage's `configSchema`, so the launcher no longer parses npm manifests at runtime. The standalone `lan-config packages` / `lan-config apply` binaries (scanning `~/.lania/config/*.json`) also work, but `lan` is the primary path. See the [configuration guide](./docs-site/guide/configuration.md) and the [config field reference](./docs-site/reference/config.md).

## Quick Start

```bash
lan --help
lan dev
lan tools --plain --names-only --unique
lan tools run ./scripts/demo.py arg1 arg2
lan tools run ./scripts/demo.rs
lan tools run ./scripts/demo.go
lan tools view ./src/index.ts
lan workspace create --name demo-app --template spa-react
lan workspace create --name svc --template go-service --package-manager go
lan workspace template
lan release plan --profile package --version 1.2.3
```

`generate` commands (`generate api` / `generate module` / `lan product generate`) have been archived and will be redesigned in a later release. The complete pre-removal code is preserved on the local branch `archive/generate-command` (pointing at commit `61d47b9`) for recovery.

Plan a product package release:

```bash
lan product build
lan product pack
lan product publish --dist-tag next --channel next
```

Execute a `publish-manifest` dry run:

```bash
lan product publish --execute --dry-run
```

Publish to a real registry:

```bash
NODE_AUTH_TOKEN=xxxxx node ./scripts/configure-npm-auth.mjs \
  --manifest ./.lania/publish/product/npm-package/publish-manifest.json \
  --output ./.lania/tmp/npmrc.publish

NPM_CONFIG_USERCONFIG=./.lania/tmp/npmrc.publish \
lan product publish --execute --yes \
  --max-retries 2 \
  --retry-delay-ms 2000
```

Run a local registry rehearsal first:

```bash
pnpm publish:verdaccio
```

GitHub Actions entry points:

- `Publish Product`: runs `lan product build`, `lan product pack`, and `lan product publish`
- `Publish Manifest`: executes an existing `publish-manifest.json` in a controlled workflow
- `Publish Verdaccio Smoke`: runs the automated Verdaccio publish rehearsal

## `tools list`

`lan tools` / `lan tools list` list commands that are directly resolvable from the current terminal environment.

- By default it includes:
  - executables and scripts discovered from `PATH`
  - shell builtins
  - shell aliases
  - shell functions
- Shell discovery currently prefers `zsh` and `bash`; unsupported shells degrade to `PATH`-only enumeration.
- Common options:
  - `--filter <text>`: substring match on command name
  - `--all-matches`: expand duplicate command names found in multiple PATH directories
  - `--no-shell`: disable shell builtin/alias/function discovery
  - `--names-only`: keep only the command names
  - `--group-by-source`: split output into PATH / builtin / alias / function groups
  - `--plain`: render plain text instead of structured JSON
  - `--unique`: deduplicate names while preserving first-seen order

## `tools run`

`lan tools run <file> [args...]` detects the target runtime and executes the file locally.

- It first respects a shebang if the file declares one.
- Supported shebang styles include:
  - `#!/usr/bin/env python3`
  - `#!/usr/bin/env -S node --no-warnings`
  - `#!/usr/bin/env FOO=bar python3 -u`
- JS and TS runtimes prefer project-local `node_modules/.bin` before falling back to the system `PATH`.
- Without a shebang it falls back to extension-based runtime selection:
  - `js` / `mjs` / `cjs` -> prefer `node`, fallback `bun`
  - `ts` / `tsx` / `jsx` -> prefer `tsx`, fallback `bun`, `ts-node`
  - `py` -> prefer `python3`, fallback `python`
  - `sh` / `bash` / `zsh` -> prefer the matching shell, fallback `bash`, `sh`
  - `rb` -> `ruby`
  - `php` -> `php`
  - `go` -> `go run`
  - `java` -> `java <file>`
  - `lua` -> `lua`
  - `dart` -> `dart run`
  - `nim` -> `nim r`
  - `zig` -> `zig run`
  - `kt` / `kts` -> prefer `kotlin`, fallback `kotlinc -script`
  - `swift` -> `swift`
- Compiled single-file languages currently use a prepare step before execution:
  - `rs` -> `rustc <file> -o <tmp-bin>`
  - `c` -> `cc/clang/gcc <file> -o <tmp-bin>`
- If the prepare step fails, the command returns the compiler error directly and does not continue to execute a missing binary.

## `tools view`

`lan tools view <path>` shows local files with a sensible default viewer.

- Text files are rendered inline with line numbers, file size, and the effective line range.
- Text viewing options:
  - `--lines <N>` limits the maximum number of rendered lines
  - `--head <N>` shows the first N lines
  - `--start <N>` starts from a specific 1-based line
  - `--end <N>` stops at a specific 1-based line
  - `--tail <N>` shows the last N lines
- `--grep <text>` filters visible text lines or directory entries by substring
- `--regex <pattern>` filters visible text lines or directory entries by regular expression
- `--ignore-case` enables case-insensitive substring or regex matching
- Directory paths are rendered as inline listings and support `--head`, `--tail`, and `--grep`
- `--tree` renders directories as a recursive tree
- `--files-only` keeps only file entries when viewing directories
- `--dirs-only` keeps only directory entries when viewing directories
- Tree filtering preserves parent directory context when a nested child matches
- `--max-depth <N>` limits recursive directory traversal depth
- `--sort name|size|time|ext` sorts directory entries by name, size, modified time, or extension
- `--reverse` reverses directory sort order
- `--hidden` includes hidden files and directories
- `--tree` now renders directories with tree-style connector lines
- Images, audio, video, and PDF files are classified before opening with the system default application.
- Unknown binary files default to an inline hex preview instead of launching an external viewer.
- `--hex-bytes <N>` limits the binary hex preview size
- `--meta-only` shows file metadata without launching the external viewer.

## Contributor Setup

The Rust workspace and the pnpm workspace both live at the repository root:

```bash
pnpm install
pnpm -r build                    # build all TS packages (bridge-runtime, slices, templates, ...)
cargo build --workspace --bins   # build all Rust binaries (lan-<cmd>, lan-runtime, ...)
cargo test --workspace
node packages/distribution/core/npm/core/bin/lan.mjs --help
```

## Dynamic Commands

Dynamic commands are enabled from project config via `extensions.dynamicCommands` in `lan.config.*`.

At startup:

1. Rust loads the project config.
2. Rust asks Node Bridge to resolve command manifests.
3. Node scans `lania.schemas.*` and `.lania/schemas/`.
4. Rust registers the generated command specs and bridge handlers.
5. Execution is forwarded back to Node Bridge through `command.invokeDynamic`.

This keeps the host runtime stable while allowing project-level command extension.

## Distribution Layout

The CLI is distributed as a Rust binary plus a colocated Node Bridge payload.

```text
<install-root>/
  bin/
    lan                    # launcher (@lania-cli-v2/core)
    lan-<cmd>              # per-command subpackage binaries
  lib/
    bridge/                # "core + slices" payload (unified since phase 7a)
      package.json
      dist/
      node_modules/        # bridge-runtime + bridge-slice-* + templates + agent
```

Bridge lookup order:

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/bridge` (since phase 7a the payload is assembled as core + slices)
3. `<current-exe>/../lib/node-bridge` (legacy layout, kept for already-installed versions)
4. `<current-exe>/../node-bridge`
5. workspace development path `packages/foundation/bridge-runtime/ts`
