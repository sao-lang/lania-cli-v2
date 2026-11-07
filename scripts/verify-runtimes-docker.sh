#!/usr/bin/env bash
# 宿主机入口：用 Docker 里的完整 Rust 工具链验证自管运行时。
#
#   ./scripts/verify-runtimes-docker.sh                    # 全量 L0-L5
#   VERIFY_INSTALL=0 ./scripts/verify-runtimes-docker.sh   # 只列版本，不真实下载
#   VERIFY_ONLINE=0 ./scripts/verify-runtimes-docker.sh    # 只跑离线单测与 CLI/TUI/API
#   VERIFY_LANGS="gleam d" ./scripts/verify-runtimes-docker.sh  # 只验证指定运行时
#
# 需要先手动启动 Docker Desktop（不要从受沙箱限制的终端里启动它）。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="${RUST_IMAGE:-rust:latest}"
CACHE_VOL="lania-rust-cargo-cache"
TARGET_VOL="lania-rust-target"

if ! docker info >/dev/null 2>&1; then
  echo "Docker daemon 未运行：请先从开始菜单/桌面手动启动 Docker Desktop，再执行本脚本。" >&2
  exit 1
fi

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "拉取镜像 $IMAGE ..."
  if ! docker pull "$IMAGE"; then
    echo "镜像 $IMAGE 不存在，回退到 rust:latest" >&2
    IMAGE="rust:latest"
    docker pull "$IMAGE"
  fi
fi

# Git Bash 下 docker -v 需要 Windows 风格路径；关闭 MSYS 自动路径转换。
HOST_ROOT="$(cd "$REPO_ROOT" && pwd -W 2>/dev/null || echo "$REPO_ROOT")"
HOST_ROOT="${HOST_ROOT//\\//}"

# 宿主机代理通常监听 127.0.0.1，容器内要换成 host.docker.internal。
PROXY_ARGS=()
p="${HTTPS_PROXY:-${https_proxy:-${HTTP_PROXY:-${http_proxy:-}}}}"
if [ -n "$p" ]; then
  p="${p//127.0.0.1/host.docker.internal}"
  p="${p//localhost/host.docker.internal}"
  PROXY_ARGS=(-e "HTTPS_PROXY=$p" -e "HTTP_PROXY=$p" -e "https_proxy=$p" -e "http_proxy=$p")
  echo "容器内代理: $p"
fi
# 放行回环地址：runex-core 单测的本地 HTTP server 不能被代理 CONNECT 掉。
NO_PROXY_ARGS=(
  -e "NO_PROXY=127.0.0.1,localhost,::1"
  -e "no_proxy=127.0.0.1,localhost,::1"
)

# 可选：宿主预先下载好的真实产物缓存（按 runex cache_path_for 命名）。
PRESEED_ARGS=()
if [ -d /tmp/lania-preseed ]; then
  PRESEED_HOST="$(cd /tmp/lania-preseed && pwd -W)"
  PRESEED_HOST="${PRESEED_HOST//\\//}"
  PRESEED_ARGS=(-v "$PRESEED_HOST:/preseed:ro")
  echo "预置缓存: $PRESEED_HOST"
fi

TTY=()
[ -t 0 ] && TTY=(-it)

echo "仓库: $HOST_ROOT"
echo "镜像: $IMAGE  在线探测: ${VERIFY_ONLINE:-1}  真实安装: ${VERIFY_INSTALL:-1}"
echo "范围: ${VERIFY_LANGS:-全部 18 种自管运行时}"

MSYS_NO_PATHCONV=1 docker run --rm "${TTY[@]}" \
  --add-host=host.docker.internal:host-gateway \
  -v "$HOST_ROOT:/workspace" \
  -v "$CACHE_VOL:/usr/local/cargo/registry" \
  -v "$TARGET_VOL:/workspace/target" \
  -w /workspace \
  "${PROXY_ARGS[@]}" \
  "${NO_PROXY_ARGS[@]}" \
  "${PRESEED_ARGS[@]}" \
  -e "VERIFY_ONLINE=${VERIFY_ONLINE:-1}" \
  -e "VERIFY_INSTALL=${VERIFY_INSTALL:-1}" \
  -e "VERIFY_LANGS=${VERIFY_LANGS:-}" \
  -e "VERIFY_REAL_LANGS=${VERIFY_REAL_LANGS-node d zig}" \
  -e "RUNEX_HTTP_TIMEOUT_SECS=${RUNEX_HTTP_TIMEOUT_SECS:-180}" \
  -e "RUNEX_DOWNLOAD_TIMEOUT_SECS=${RUNEX_DOWNLOAD_TIMEOUT_SECS:-900}" \
  "$IMAGE" \
  bash /workspace/scripts/verify-runtimes.sh
