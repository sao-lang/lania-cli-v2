# Lania CLI v2

[English](./README.md) | [简体中文](./README.zh-CN.md)

`Lania CLI v2` is a dual-runtime CLI: Rust owns the host runtime, command graph, workflows, and unified output model; Node Bridge owns JavaScript ecosystem integration, config loading, templates, and runtime extensions.

## Documentation

- [Chinese README](./README.zh-CN.md)
- [Docs Index](./docs/README.zh-CN.md)
- [Learning Guide](./docs/guides/learning-map.zh-CN.md)
- [Runtime Architecture](./docs/architecture/runtime-architecture.zh-CN.md)
- [Framework And Product Overview](./docs/architecture/框架化与产品化总览.zh-CN.md)
- [Design Notes And Reconciliation](./docs/design/专题设计与对账.zh-CN.md)
- [Node Bridge Protocol](./docs/archive/phase0/node-bridge-protocol.md)

## Current Capabilities

- Built-in commands: `dev`, `build`, `lint`, `tools`, `create`, `add`, `template`, `generate api`, `generate module`, `release`, `sync`, `config`
- Bridge-backed execution for compiler and lint workflows, plus mixed host/bridge tooling utilities under `tools`
- Rust workflows for scaffolding, code generation, release orchestration, and Git automation
- Project-level dynamic commands loaded from manifest files
- Project plugins with source constraints, allowlists, and method restrictions
- Config support for `lan.config.js`, `lan.config.cjs`, `lan.config.json`, `lan.config.ts`
- Output modes: `json`, `jsonl`, `human`

## Architecture

- `rust/`: host runtime, command parsing, workflows, prompt, progress, logging, exec, git, filesystem, task services
- `ts/`: Node Bridge, plugin registry, config loaders, template runtime, JS tooling integration
- `npm/`: installable wrapper that resolves the platform binary and injects the bridge payload path

In practice:

- Rust decides what command exists and how it is executed.
- Node decides how JS-native capabilities are loaded and invoked.
- The npm wrapper is responsible for installation-time ergonomics.

## Command Categories

| Command Group | Path | Description |
| --- | --- | --- |
| `dev`, `build`, `lint` | Rust -> Node Bridge -> compiler/lint plugins | JS toolchain execution |
| `tools list` | Rust -> Node Bridge -> system plugin | enumerate terminal-resolvable commands from PATH and shell |
| `tools run`, `tools view` | Rust host utilities | detect runtimes, execute local code files, or display/open local files |
| `create`, `add` | Rust workflow + template bridge calls | project and partial scaffolding |
| `generate api`, `generate module` | Rust workflow | file generation, manifests, optional injection |
| `release`, `sync` | Rust workflow | release orchestration and Git automation |
| `template` | Rust query -> Node Bridge | template catalog and template detail lookup |
| dynamic commands | project manifest -> Rust registration -> Node execution | runtime command extension |

## Quick Start

```bash
lan --help
lan dev
lan tools --plain --names-only --unique
lan tools run ./scripts/demo.py arg1 arg2
lan tools run ./scripts/demo.rs
lan tools run ./scripts/demo.go
lan tools view ./src/index.ts
lan create --name demo-app --template spa-react
lan template
lan generate api init
lan release plan --profile package --version 1.2.3
```

Plan a product package release:

```bash
lan build product
lan pack product
lan publish product --dist-tag next --channel next
```

Execute a `publish-manifest` dry run:

```bash
lan publish product --execute --dry-run
```

Publish to a real registry:

```bash
NODE_AUTH_TOKEN=xxxxx node ./scripts/configure-npm-auth.mjs \
  --manifest ./.lania/publish/product/npm-package/publish-manifest.json \
  --output ./.lania/tmp/npmrc.publish

NPM_CONFIG_USERCONFIG=./.lania/tmp/npmrc.publish \
./rust/target/debug/lania-cli publish product \
  --execute \
  --yes \
  --max-retries 2 \
  --retry-delay-ms 2000
```

Run a local registry rehearsal first:

```bash
pnpm publish:verdaccio
```

GitHub Actions entry points:

- `Publish Product`: runs `lan build product`, `lan pack product`, and `lan publish product`
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

```bash
cd ts
pnpm install
pnpm build
cd ../rust
cargo build -p lania-cli
cargo test -p lania-cli
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
    lan
    lania-cli
  lib/
    node-bridge/
      package.json
      dist/
      node_modules/
```

Bridge lookup order:

1. `LANIA_NODE_BRIDGE_DIR`
2. `<current-exe>/../lib/node-bridge`
3. `<current-exe>/../node-bridge`
4. `<current-exe>/../Resources/node-bridge`
5. workspace development path `ts/packages/node-bridge`
