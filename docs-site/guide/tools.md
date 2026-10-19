# 工具命令 tools

<div class="lead">
tools 是 Lania CLI 中最像“瑞士军刀”的命令族：它既能枚举当前终端可运行的命令，也能按文件类型自动选择运行时执行代码，还能直接在终端查看文本、目录或调用系统查看器打开媒体文件。
</div>

## 命令结构

```text
lan tools
  list    枚举终端可解析命令
  run     按文件类型执行代码
  view    查看文本、目录或媒体文件
```

直接执行 `lan tools` 等价于 `lan tools list`。

## tools list

### 基本用法

```bash
lan tools
lan tools list
lan tools --plain --names-only --unique
```

默认返回：

- `PATH` 中的可执行文件与脚本
- Shell builtin
- Shell alias
- Shell function

Shell 采集优先支持 `zsh` / `bash`；其他 shell 会降级为只枚举 `PATH`。

### 常用参数

| 参数                | 作用                                      |
| ------------------- | ----------------------------------------- |
| `--filter <text>`   | 按命令名子串过滤                          |
| `--all-matches`     | 展开同名命令的多个 PATH 命中              |
| `--no-shell`        | 只返回 PATH 命令                          |
| `--names-only`      | 只返回命令名                              |
| `--group-by-source` | 按 PATH / builtin / alias / function 分组 |
| `--plain`           | 输出纯文本                                |
| `--unique`          | 按首次出现顺序去重                        |
| `--limit <n>`       | 限制返回数量                              |
| `--shell`           | 包含 shell builtin / alias / function     |

### 示例

```bash
# 查找包含 ts 的命令
lan tools --filter ts

# 查看 node 在 PATH 中的所有匹配
lan tools --all-matches --filter node

# 纯文本输出，适合管道
lan tools --plain --names-only --unique --limit 20
```

## tools run

### 基本用法

```bash
lan tools run <file> [args...]
```

执行规则：

1. 优先读取文件头 shebang。
2. 没有 shebang 时按扩展名选择运行时。
3. JS / TS 运行时优先查找项目本地 `node_modules/.bin`。
4. Windows 下会按 `PATHEXT` 解析 `.exe` / `.cmd` / `.bat`，并跳过不可执行的 Unix shim。

### 运行时选择

| 扩展名                | 运行时优先级                      |
| --------------------- | --------------------------------- |
| `js` / `mjs` / `cjs`  | `node` → `bun`                    |
| `ts` / `tsx` / `jsx`  | `tsx` → `bun` → `ts-node`         |
| `py`                  | `python3` → `python`              |
| `sh` / `bash` / `zsh` | 对应 shell → `bash` → `sh`        |
| `rb`                  | `ruby`                            |
| `php`                 | `php`                             |
| `go`                  | `go run`                          |
| `java`                | `java <file>`                     |
| `lua`                 | `lua`                             |
| `dart`                | `dart run`                        |
| `nim`                 | `nim r`                           |
| `zig`                 | `zig run`                         |
| `kt` / `kts`          | `kotlin` → `kotlinc -script`      |
| `swift`               | `swift`                           |
| `rs`                  | `rustc <file> -o <tmp>` 后执行    |
| `c`                   | `cc` / `clang` / `gcc` 编译后执行 |

### shebang

支持：

```text
#!/usr/bin/env python3
#!/usr/bin/env -S node --no-warnings
#!/usr/bin/env FOO=bar python3 -u
```

### 示例

```bash
lan tools run ./scripts/demo.py
lan tools run ./scripts/demo.ts -- --port 3000
lan tools run ./scripts/demo.go arg1 arg2
lan tools run ./scripts/demo.rs
```

## tools view

### 基本用法

```bash
lan tools view <path>
```

- 文本文件：内联显示内容、行号、字节大小与行范围。
- 目录：内联列出目录项。
- 图片 / 音频 / 视频 / PDF：识别类型后调用系统默认查看器。
- 未知二进制：输出十六进制预览。

### 文本查看参数

| 参数                | 作用                           |
| ------------------- | ------------------------------ |
| `--lines <n>`       | 最多显示多少行                 |
| `--head <n>`        | 显示前 n 行                    |
| `--start <n>`       | 从第 n 行开始                  |
| `--end <n>`         | 到第 n 行结束                  |
| `--tail <n>`        | 显示最后 n 行                  |
| `--grep <text>`     | 子串过滤                       |
| `--regex <pattern>` | 正则过滤                       |
| `--ignore-case`     | 忽略大小写                     |
| `--hex-bytes <n>`   | 限制二进制十六进制预览字节数   |
| `--meta-only`       | 只显示元信息，不打开外部查看器 |

### 目录查看参数

| 参数                           | 作用         |
| ------------------------------ | ------------ |
| `--tree`                       | 递归树形展示 |
| `--files-only`                 | 只保留文件   |
| `--dirs-only`                  | 只保留目录   |
| `--max-depth <n>`              | 限制递归深度 |
| `--sort name\|size\|time\|ext` | 排序方式     |
| `--reverse`                    | 反转排序     |
| `--hidden`                     | 显示隐藏项   |

### 示例

```bash
lan tools view ./src/index.ts
lan tools view ./src/index.ts --head 40
lan tools view ./logs/app.log --grep error --ignore-case
lan tools view ./src --tree --files-only
lan tools view ./assets/logo.png
lan tools view ./build/app.bin --hex-bytes 128
```

## 下一步

- [命令总览](/guide/commands)
- [输出、语言与进度](/guide/output-and-locale)
- [常见问题](/guide/troubleshooting)
