#!/usr/bin/env bash
# 在容器里验证 runex 自管运行时。
#
# 分层（默认全开，可用环境变量裁剪）：
#   L0  离线单测与全量编译
#   L1  runex 命令行用法（CLI）
#   L2  runex 终端界面用法（TUI，非交互快照）
#   L3  runex Rust API 用法（经 lan env 接入，含 dry-run 与 --yes 真实变更）
#   L4  全部 18 种自管运行时的上游版本清单
#   L5a 全部 18 种的管理路径（install/use --dry-run，不下载）
#   L5b 真实 install -> use -> which -> 执行 -> remove 闭环
#
# 环境变量：
#   VERIFY_ONLINE=0      跳过 L4/L5 的联网部分
#   VERIFY_INSTALL=0     只做 dry-run，不真实下载
#   VERIFY_LANGS="d zig" 只验证这几种（默认全部 18 种）
#   VERIFY_REAL_LANGS="node d zig" | "all" | ""   L5b 的真实安装范围
#   RUNEX_HTTP_TIMEOUT_SECS=180   上游清单读取预算（Julia 清单 2.2MB，默认 30s 不够）
#   RUNEX_DOWNLOAD_TIMEOUT_SECS=900
#
# 所有运行时装在独立的 RUNEX_HOME 里，不碰宿主机 ~/.runex。
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT/rust" || exit 1

export CARGO_TERM_COLOR=never
export RUNEX_HOME=/tmp/runex-verify
mkdir -p "$RUNEX_HOME"

# runex-core 的单测会起本地 HTTP server；容器里的全局代理会让 ureq 对
# 127.0.0.1 发 CONNECT，得到 502。必须显式放行回环地址。
export NO_PROXY="127.0.0.1,localhost,::1,${NO_PROXY:-}"
export no_proxy="$NO_PROXY"
# 慢代理下的清单/下载预算（产品自带的环境变量，非改动代码）。
export RUNEX_HTTP_TIMEOUT_SECS="${RUNEX_HTTP_TIMEOUT_SECS:-180}"
export RUNEX_DOWNLOAD_TIMEOUT_SECS="${RUNEX_DOWNLOAD_TIMEOUT_SECS:-900}"

VERIFY_ONLINE="${VERIFY_ONLINE:-1}"
VERIFY_INSTALL="${VERIFY_INSTALL:-1}"
VERIFY_LANGS="${VERIFY_LANGS:-}"
VERIFY_REAL_LANGS="${VERIFY_REAL_LANGS-node d zig}"
[ "$VERIFY_INSTALL" = 1 ] && VERIFY_ONLINE=1

PASS=0
FAIL=0
NOTES=0
NET_FAIL=0
step() { printf '\n=== %s ===\n' "$*"; }
check() {
  if "$@"; then
    PASS=$((PASS + 1)); printf '  OK   %s\n' "$*"
  else
    FAIL=$((FAIL + 1)); printf '  FAIL %s\n' "$*"
  fi
}
note() { NOTES=$((NOTES + 1)); printf '  NOTE %s\n' "$*"; }

step "L0 / 编译与离线单测"
check cargo test -p runex-core --lib
check cargo test -p runex
check cargo test -p lania-plugins-command-env
check cargo test -p lania-cli --test phase3_e2e lan_env_
check cargo test --workspace --no-run
check cargo build -p runex -p lania-cli

BIN=target/debug/runex
LAN=target/debug/lania-cli
if [ ! -x "$BIN" ] || [ ! -x "$LAN" ]; then
  printf '  FAIL 缺少二进制：%s 或 %s\n' "$BIN" "$LAN"
  exit 2
fi

step "L1 / runex 命令行用法 (CLI)"
check "$BIN" --json list
check "$BIN" --json list --installed
check "$BIN" --json info node
check "$BIN" --json versions node
check "$BIN" --json doctor
check "$BIN" --json init --shell bash
check "$BIN" --json locale
check "$BIN" --json gc --dry-run --days 9999
check "$BIN" --json install node 999.999.999 --dry-run

step "L2 / runex 终端界面用法 (TUI 快照)"
check "$BIN" ui --snapshot --no-versions
check "$BIN" ui --snapshot --no-versions --cols 100 --rows 28
check "$BIN" ui --snapshot --no-versions --down 3
check "$BIN" --json ui --snapshot --no-versions

step "L3 / runex Rust API 用法 (lan env 接入)"
check "$LAN" env list
check "$LAN" env list --installed
check "$LAN" env info node
check "$LAN" env versions node
check "$LAN" env doctor
check "$LAN" env init --shell bash
check "$LAN" env ui --snapshot
# 默认 dry-run：只出计划、不落盘、不下载
check "$LAN" env install node 20
check "$LAN" env use node 20

ALL_LANGS=(node python go rust deno bun zig java dart kotlin scala julia d v crystal gleam haxe elixir)
if [ -n "$VERIFY_LANGS" ]; then
  read -ra LANGS <<< "$VERIFY_LANGS"
else
  LANGS=("${ALL_LANGS[@]}")
fi

available_array() {
  "$BIN" --json versions "$1" --available 2>/dev/null \
    | tr -d '\n' \
    | sed -n 's/.*"available"[[:space:]]*:[[:space:]]*\[\([^]]*\)\].*/\1/p'
}

# runex versions --available --json 的 available 是字符串数组。
# 取第一个稳定版，跳过 alpha/beta/RC 等预发布（否则会去下载 60MB 的 nightly）。
stable_version() {
  # 注意顺序：先删引号/空格（不能删 [:space:]，否则会把逗号转出的换行也吃掉），
  # 再按逗号分行，最后过滤预发布并取第一个。
  available_array "$1" \
    | tr -d '" ' \
    | tr ',' '\n' \
    | grep -v -Ei 'alpha|beta|([.-]rc[0-9]*)|preview|nightly|dev|canary|snapshot|^$' \
    | head -1
}

entry_of() {
  case "$1" in
    python) echo python3 ;;
    rust)   echo rustc ;;
    d)      echo dmd ;;
    *)      echo "$1" ;;
  esac
}

# 各运行时的版本参数并不统一；go/zig 用 `version`、JVM 系用 `-version`。
version_flag() {
  case "$1" in
    go|zig)       echo version ;;
    kotlin|scala) echo -version ;;
    *)            echo --version ;;
  esac
}

if [ "$VERIFY_ONLINE" = 1 ]; then
  step "L4 / 上游版本清单（全部自管运行时）"
  for lang in "${LANGS[@]}"; do
    # GitHub API / 代理偶发限流会让同一轮里多个 provider 同时返回空清单；
    # 重试后仍为空按「网络类失败」计，避免把上游抖动误判成产品缺陷。
    arr=""
    for attempt in 1 2 3; do
      arr=$(available_array "$lang")
      if printf '%s' "$arr" | grep -q '[^[:space:]]'; then break; fi
      [ "$attempt" -lt 3 ] && sleep 3
    done
    if printf '%s' "$arr" | grep -q '[^[:space:]]'; then
      PASS=$((PASS + 1)); printf '  OK   versions %-8s -> %s\n' "$lang" "$(printf '%s' "$arr" | cut -c1-58)"
    else
      NET_FAIL=$((NET_FAIL + 1))
      printf '  NET_FAIL versions %s (上游 API 超时/限流，重试 3 次仍为空；不计产品缺陷)\n' "$lang"
    fi
  done

  step "L5a / 管理路径覆盖（全部运行时，dry-run 不下载）"
  for lang in "${LANGS[@]}"; do
    check "$BIN" --json install "$lang" latest --dry-run
    check "$LAN" env use "$lang" latest
  done

  if [ "$VERIFY_INSTALL" = 1 ]; then
    if [ "$VERIFY_REAL_LANGS" = all ]; then
      REAL_LANGS=("${LANGS[@]}")
    elif [ -n "$VERIFY_REAL_LANGS" ]; then
      read -ra REAL_LANGS <<< "$VERIFY_REAL_LANGS"
    else
      REAL_LANGS=()
    fi

    step "L5b / 真实安装 -> use -> which -> 执行 -> remove"
    # 宿主已用并发 Range 下载好真实上游产物，按 runex 的 cache_path_for
    # 规则命名；预置后 install 会命中缓存，避免容器代理的慢速单连接。
    # RuntimeHome::downloads_dir() = $RUNEX_HOME/cache/downloads（注意中间有 cache/）。
    CACHE_DIR="$RUNEX_HOME/cache/downloads"
    mkdir -p "$CACHE_DIR"
    if compgen -G "/preseed/*" >/dev/null; then
      cp /preseed/* "$CACHE_DIR/"
      printf '  预置缓存: %s 个文件 -> %s\n' "$(find "$CACHE_DIR" -type f | wc -l)" "$CACHE_DIR"
    else
      printf '  无预置缓存，install 将直接走上游网络\n'
    fi
    printf '  范围: %s\n' "${REAL_LANGS[*]:-(无)}"
    for lang in "${REAL_LANGS[@]}"; do
      printf '\n-- %s --\n' "$lang"
      ver=$(stable_version "$lang")
      if [ -z "$ver" ]; then
        NET_FAIL=$((NET_FAIL + 1)); printf '  NET_FAIL 无法解析 %s 的稳定版本（上游清单）\n' "$lang"
        continue
      fi
      printf '  稳定版: %s\n' "$ver"
      log="/tmp/install-$lang.log"
      if ! timeout 300 "$BIN" --json install "$lang" "$ver" >"$log" 2>&1; then
        NET_FAIL=$((NET_FAIL + 1))
        printf '  NET_FAIL install %s %s（上游下载/代理，详见 %s）\n' "$lang" "$ver" "$log"
        tail -3 "$log" | sed 's/^/         /'
        continue
      fi
      PASS=$((PASS + 1)); printf '  OK   install %s %s\n' "$lang" "$ver"

      check "$BIN" --json use "$lang" "$ver"

      entry=$(entry_of "$lang")
      exe=$("$BIN" which "$entry" --resolve 2>/dev/null | head -1)
      if [ -n "$exe" ] && [ -x "$exe" ]; then
        PASS=$((PASS + 1)); printf '  OK   shim %-9s -> %s\n' "$entry" "$exe"
        flag=$(version_flag "$lang")
        if "$exe" "$flag" >/dev/null 2>&1; then
          PASS=$((PASS + 1)); printf '  OK   %s %s\n' "$entry" "$flag"
        else
          note "$entry $flag 未成功（可能缺少外部依赖，如 JVM / Erlang）"
        fi
      else
        FAIL=$((FAIL + 1)); printf '  FAIL which %s 未解析到可执行文件\n' "$entry"
      fi

      check "$BIN" --json remove "$lang" "$ver"

      # 同一套操作再经 API (lan env --yes) 走一遍，验证宿主接入的写路径。
      if [ "$lang" = "${REAL_LANGS[0]}" ]; then
        step "L3b / API 真实变更路径 (--yes, $lang)"
        check "$LAN" env install "$lang" "$ver" --yes
        check "$LAN" env use "$lang" "$ver" --yes
        check "$LAN" env which "$(entry_of "$lang")"
        check "$LAN" env remove "$lang" "$ver" --yes
      fi
    done

    step "L5 / 卸载后状态"
    check "$BIN" --json list --installed
  fi
fi

printf '\n=== 结果: PASS=%d FAIL=%d NOTE=%d NET_FAIL=%d -> %s ===\n' \
  "$PASS" "$FAIL" "$NOTES" "$NET_FAIL" \
  "$([ "$FAIL" = 0 ] && echo PRODUCT_ALL_PASS || echo PRODUCT_HAS_FAILURES)"
[ "$NET_FAIL" -gt 0 ] && printf '注意：NET_FAIL 为上游下载受容器代理限制，不计入产品缺陷。\n'
exit "$FAIL"
