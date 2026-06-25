# product-cli-demo

[简体中文](./README.zh-CN.md)

Minimal example product CLI workspace for `generate/build/pack/publish`.

This example shows how to author a product CLI that:

- exposes product commands from `product/lania.schemas.ts`
- includes an interactive-style `deploy` command with options and prompts
- bundles product templates from `product/templates`
- can be validated through `lan product build`, `lan product pack`, and `lan product publish`

## Files

- `lan.config.json`: product config, compat ranges, and minimal tool policy
- `product/lania.schemas.ts`: product commands and workflows
- `product/templates/demo-app/template.json`: product-owned template metadata
- `product/templates/demo-app/files/README.md.ejs`: rendered template asset

## Quick Start

From this example directory:

```bash
lan product inspect --path .
lan product dev hello --path .
lan product dev deploy --path . -- --service api --env staging --confirm
lan product build
lan product pack
lan product publish
```

If you want to validate the publish manifest execution path:

```bash
lan product publish --execute --dry-run --yes
```

Note:

- `--execute --dry-run` still performs registry preflight checks such as `npm whoami`
- if npm auth is not configured locally, prefer `lan product publish` or `pnpm publish:verdaccio`

## Expected Results

- `lan product inspect --path .` shows product diagnostics in human mode
- `lan product dev hello --path .` runs the local product command in development mode
- `lan product dev deploy --path . -- --service api --env staging --confirm` writes `.lania/reports/deploy-plan.json`
- `lan product publish` writes `.lania/publish/product/npm-package/publish-manifest.json`
