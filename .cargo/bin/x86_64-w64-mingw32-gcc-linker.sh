#!/usr/bin/env bash
# 本机 linker wrapper（不进版本库）：rustc 调用 dlltool/ld 时按进程 PATH 查找，
# 而 rustup 自带的 self-contained MinGW 目录不在 PATH 中，故在此注入。
set -e
SC="$HOME/.rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained"
export PATH="$SC:$PATH"
exec "$SC/x86_64-w64-mingw32-gcc.exe" "$@"
