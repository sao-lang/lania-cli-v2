# Directory Structure Reference

This document describes the current directory structure after the package-split refactor (phase 1-9 completed). The legacy top-level `rust/`, `ts/`, and `npm/` directories no longer exist; all capabilities now live under `packages/`.

## Top-level

```text
lania-cli-v2/
  packages/                 # All capability packages (commands, distribution, foundation, products, runtime, schemas)
  scripts/                  # Release/pack/publish/version/manifest-check utilities
  tests/                    # Golden tests and shared test fixtures
  docs/                     # Design notes, guides, architecture and operations docs
  examples/                 # Example projects for local verification
  docs-site/                # Documentation site assets
  Cargo.toml                # Rust workspace root (at repo root, not a rust/ subdirectory)
  package.json              # pnpm workspace root
  .changeset/               # Changesets metadata (JS package versioning)
  .ai/                      # Reference docs for contributors (this directory)
  .vscode/                  # Local editor defaults (optional)
```

## `packages/` layout

Every package ships its own `lania.package.json` manifest (with a `description` field), and contains a `rust/` subdirectory, an `npm/` subdirectory, and a `describes/` subdirectory where applicable.

### `packages/commands/<cmd>/`

One subpackage per built-in command. There are 11 built-in commands: `dev`, `build`, `lint`, `create`, `add`, `release`, `git`, `tools`, `env`, `pi`, `config`.

- Each `packages/commands/<cmd>/` contains:
  - `rust/`: the command plugin crate (parameter parsing and orchestration entry)
  - `npm/cmd-<cmd>/` and platform binaries `cmd-<cmd>-<platform>/`: the per-command installable binary package
  - `describes/`: command description assets
  - `lania.package.json`: package manifest, including the `configSchema` for the command's `config` subcommand
- `template` is a second-level command of `create` (not a standalone command).
- `product` is a shared second-level namespace of the `dev`/`build` packages.
- The former `generate` command has been deleted and archived to the local branch `archive/generate-command` (to be re-enabled after a redesign).

### `packages/distribution/`

- `core/`: the launcher. The only `lan` entry point; dispatches to per-command subpackage binaries according to `known-commands.json`.
- `cli/`: the aggregate npm package. It only statically registers `config`; the aggregated `lan --help` output is provided by the launcher.

### `packages/foundation/`

Foundational building blocks shared by command packages:

- `bridge-runtime/`: the Node bridge core (stdio protocol core). TS sources live in `bridge-runtime/ts/src`.
- `bridge-slices/<slice>/`: 12 TS bridge slices: `compiler`, `lint`, `template`, `product`, `toolchain`, `system`, `lifecycle`, `config`, `agent`, `dynamic-commands`, `commitizen`, `commitlint`. Plugin development happens in `bridge-slices/<slice>/ts/src/plugins/`.
- `templates/`: the 10 templates plus EJS rendering runtime.
- `cli-kit/`: output rendering and `template_catalog`.
- `host-core/`: host runtime core (command registration, handler registry, capability container, HookBus).
- `command/`: command abstraction (CommandSpec, parsing glue).
- `command-conformance/`: command conformance checks.
- `package-config/`: preference engine (`lania-package-config`) plus the shared `config` subcommand implementation (`lania-command-config`).
- `runex-home/`: runex home directory management.
- `capabilities/rust/`: 16 leaf service crates (`lania-config`, `lania-exec`, `lania-format`, `lania-fs`, `lania-git`, `lania-hooks`, `lania-logger`, `lania-pm`, `lania-preferences`, `lania-presentation`, `lania-progress`, `lania-prompt`, `lania-runtime`, `lania-task`, `lania-validate`, `lania-workflows`).
- `lania/`: author-side DSL.

### `packages/products/`

- `runex/`: self-managed runtime manager.
- `bridge-hub/`: language-neutral message bus with 5-language endpoints.
- `agent/`: the pi engine.

### `packages/runtime/`

`lan-runtime`: the project-level dynamic command host.

### `packages/schemas/`

Shared JSON schema for `lania.package.json` (`lania.package.schema.json`).

## Bridge payload locations

The Node Bridge is now a "core + slices" architecture:

- Published packages: the bridge payload lives in each package's `lib/bridge` directory.
- Development state: the bridge payload lives in `packages/foundation/bridge-runtime/ts`.
- Legacy references to `ts/packages/node-bridge` must be replaced by the paths above.

## Config layers

Config is resolved across three layers:

1. Project-level `lan.config.*`
2. Global `~/.lania/preferences.json`
3. Package-level `~/.lania/config/<pkg>.json`

All 10 command subpackages that support configuration ship their own `config` second-level command (`bare`/`get`/`set`/`init`/`doctor`/`sync` plus `--global`).

## Conventions

- Generated artifacts must live under `dist/` (TS) or `target/` (Rust); platform binary payloads under `npm/cmd-*-<platform>/bin/` and `npm/cli-<platform>/bin/` are gitignored.
- User-facing release version is kept consistent across Rust + JS packages via `changesets`.
- Root scripts are the entry points for developers (see `scripts/` for pack/publish/changeset utilities).
