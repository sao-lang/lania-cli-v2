# 发布与同步

<div class="lead">
release 负责发布规划、执行、恢复与状态查询；git 负责 Git 查询、分支/标签、暂存、提交与推送。两者都由 Rust 工作流执行，并通过宿主服务完成 Git、进程、配置与状态持久化。
</div>

## release

### 命令结构

```text
lan release [options] [command]
  plan    生成发布计划并持久化状态
  run     执行发布计划
  resume  恢复失败或部分完成的发布
  status  查看最近一次发布状态
```

### 常用选项

| 选项                      | 作用                                         |
| ------------------------- | -------------------------------------------- |
| `-v, --version <version>` | 发布版本                                     |
| `-t, --tag <tag>`         | dist-tag 覆盖                                |
| `-P, --profile <profile>` | `package` / `web-app` / `service` / `custom` |
| `-e, --env <env>`         | 目标环境                                     |
| `-C, --channel <channel>` | 发布通道                                     |
| `-f, --from <stage>`      | 从指定阶段开始                               |
| `-T, --to <stage>`        | 到指定阶段结束                               |
| `-s, --skip <stages>`     | 逗号分隔的跳过阶段                           |
| `-S, --state-file <path>` | 状态文件路径                                 |
| `-p, --publish`           | 启用 package publish 阶段                    |
| `-c, --changelog`         | 启用 changelog 阶段                          |
| `--skip-git`              | 跳过最终 Git commit/tag/push                 |
| `-a, --apply`             | 真正执行，而非只规划                         |
| `-d, --dry-run`           | 强制 plan-only                               |
| `-y, --yes`               | 免确认执行                                   |

### 示例

```bash
# 预览发布计划
lan release plan --profile package --version 1.2.3

# 预览 web-app 发布
lan release --profile web-app --env prod

# 执行指定阶段
lan release run --apply --yes --from verify --to finalize

# 查看状态
lan release status
```

### 执行链路

```text
release plan
  → 读取配置与 Git 状态
  → 构建阶段计划
  → 输出命令计划
  → 持久化 .lania/release-state.json

release run
  → 读取/恢复状态
  → 逐阶段执行
  → 记录成功/失败
  → 支持 resume
```

### 状态文件

默认状态文件：`.lania/release-state.json`。

它记录：

- 当前阶段
- 已完成阶段
- 失败信息
- 版本、profile、环境、通道
- 已执行的命令与结果

## git

`lan git` 是 Git 的统一入口：只读查询、分支/标签管理、暂存与提交推送都在同一个命令族下。
`sync` / `commit` / `push` 沿用原 `lan sync` 的语义与选项。

### 命令结构

```text
lan git <command>
  status           查看工作区状态
  log [-n 10]      查看最近提交
  diff             查看差异（--staged 只看暂存区）
  branch list      列出本地与远程分支
  branch create    创建分支
  branch switch    切换分支
  branch delete    删除分支（需 --yes）
  tag list         列出标签
  tag create       创建标签（-m 生成附注标签）
  tag delete       删除标签（需 --yes）
  add              暂存全部修改
  restore          丢弃修改（需 --yes）
  sync             暂存 + 提交 + 推送
  commit           暂存 + 提交
  push             推送当前分支
```

### 常用选项

| 选项                      | 作用                         |
| ------------------------- | ---------------------------- |
| `-r, --remote <remote>`   | 远端名，默认 `origin`        |
| `-b, --branch <branch>`   | 分支名                       |
| `-m, --message <message>` | 提交信息                     |
| `-a, --amend`             | amend 最近一次提交           |
| `-f, --force-with-lease`  | 带保护的强制推送             |
| `-d, --dry-run`           | 只规划 Git 命令              |
| `-i, --interactive`       | 通过 commitizen 生成提交信息 |
| `-p, --push`              | 提交后推送                   |

### 示例

```bash
lan git status
lan git log -n 5
lan git diff --staged
lan git branch create feature/login
lan git branch switch feature/login
lan git --dry-run sync --message "chore(sync): verify"
lan git sync --message "feat: add user api"
lan git commit --message "fix: handle empty input" --no-push
lan git push
```

### 破坏性操作

`branch delete` / `tag delete` / `restore` 默认**只输出命令计划**，加 `--yes` 才会真正执行：

```bash
lan git tag delete v0.1.0           # 只打印计划
lan git tag delete v0.1.0 --yes     # 真正删除
```

### commitlint

`sync` / `commit` 在提交前会通过 Node Bridge 调用 commitlint：

- 如果项目存在 commitlint 配置，会按配置校验。
- 没有配置时使用默认规则。
- 非规范提交信息会被拒绝，并返回原因。

## 命令计划与安全

git 写操作和 release 都会在执行前生成命令计划：

```text
git status --porcelain
git log --oneline -5
npm version 1.2.3 --no-git-tag-version
npm run build
git add -A
git commit -m "chore(release): version 1.2.3"
```

`--dry-run` 只输出计划，不执行。真正执行前请确认：

- 当前分支正确
- 工作区状态符合预期
- registry 登录态可用
- 版本号与 dist-tag 正确

## 下一步

- [产品 CLI](/guide/product-cli)
- [打包并发布产品 CLI](/cookbook/product-publish)
- [CLI 命令参考](/reference/cli)
