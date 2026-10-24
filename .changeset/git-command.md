---
'@lania-cli/cli': minor
---

新增 `lan git` 命令族：`status` / `log` / `diff` / `branch list|create|switch|delete` / `tag list|create|delete` / `add` / `restore`，以及原 `lan sync` 的 `sync` / `commit` / `push`。`lan sync` 已移除，请改用 `lan git sync`（`status` / `commit` / `push` 对应 `lan git status` / `commit` / `push`）。破坏性操作（删除分支/标签、`restore`）默认只输出命令计划，需要 `--yes` 才执行。
