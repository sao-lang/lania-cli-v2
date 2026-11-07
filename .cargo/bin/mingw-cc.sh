#!/usr/bin/env bash
# 本机 C 编译器 wrapper（不进版本库）：rustup 自带的 self-contained MinGW 目录
# 自带 as/ld/dlltool，但不在 PATH 中，gcc 找不到它们时会失败，故在此注入。
set -e
SC="$HOME/.rustup/toolchains/stable-x86_64-pc-windows-gnu/lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained"
export PATH="$SC:$PATH"
exec "$SC/x86_64-w64-mingw32-gcc.exe" "$@"
