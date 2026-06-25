# schema-tools-demo

Minimal example repository for `ctx.tools` in dynamic commands and inline hooks.

Files:

- `lan.config.ts`: enables `extensions.dynamicCommands` and shows a minimal `tools` policy.
- `lania.schemas.ts`: demonstrates one inline hook plus one handler using `text`, `bridge`, `workspace`, `pm`, `git`, `exec`, `fs`, `log`, `path`, and `result`.
- `package.json`: keeps the example repository shape realistic for `workspace.packageJson()` and `pm.detect()`.

Expected flow:

1. The inline `onArgsParsed` hook renders a styled banner with `ctx.tools.text` and pings the local bridge with `ctx.tools.bridge.call('bridge.ping')`.
2. The `inspect` handler reads workspace metadata, detects the package manager, reads the current branch, runs `/bin/echo` through `ctx.tools.exec.spawn()`, and writes a JSON report through `ctx.tools.fs.writeJson()`.
3. The command returns `ctx.tools.result.ok(...)` and emits a success log through `ctx.tools.log.success(...)`.

Expected output file:

- `.lania/reports/schema-tools.json`
