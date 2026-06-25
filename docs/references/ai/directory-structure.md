# Directory Structure Reference

This document describes the intended directory structure for this repository.

## Top-level

```text
lania-cli-v2/
  rust/                     # Rust workspace (host runtime + workflows + CLI binary)
  ts/                       # TypeScript workspace (node-bridge + templates)
  npm/                      # NPM installable CLI wrapper + platform binaries
  scripts/                  # Release/pack/publish/version utilities
  docs/                     # Design notes and distribution docs
  .changeset/               # Changesets metadata (JS package versioning)
  .ai/                      # Reference docs for contributors (this directory)
  .vscode/                  # Local editor defaults (optional)
```

## Responsibilities

- `rust/`
  - Owns the primary executable `lania-cli` (invoked by `lan` wrapper).
  - Owns the runtime contract for locating and launching the Node bridge payload.
- `ts/`
  - `ts/packages/node-bridge`: Node bridge runtime (stdio protocol + plugins).
  - `ts/packages/templates`: Template runtime and template assets.
- `npm/`
  - `npm/cli`: npm-facing wrapper that exposes `lan` and stages `lib/node-bridge` payload.
  - `npm/cli-darwin-arm64`: platform-specific Rust binary package (optional dependency).

## Conventions

- Generated artifacts must live under `dist/` (TS) or `target/` (Rust); do not commit them.
- User-facing release version is kept consistent across Rust + JS packages via `changesets`.
- Root scripts are the entry points for developers:
  - `npm run pack`
  - `npm run publish`
  - `npm run changeset:add`
  - `npm run changeset:version`

