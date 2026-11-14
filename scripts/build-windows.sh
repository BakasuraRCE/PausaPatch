#!/usr/bin/env bash
set -euo pipefail

# Cross-compile PausaPatch from Linux to Windows 11 x64.
# Requirements: rustup target x86_64-pc-windows-msvc, clang/lld, xwin.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Defaults match the standard locations; override either one through the environment.
XWIN_DIR="${XWIN_DIR:-$HOME/.xwin-cache/splat}"

if [ ! -d "$XWIN_DIR/sdk/lib/um/x86_64" ]; then
  echo "xwin SDK not found at $XWIN_DIR (set XWIN_DIR to your 'xwin splat' output)" >&2
  exit 1
fi

export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER:-lld-link}"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-Lnative=$XWIN_DIR/crt/lib/x86_64 -Lnative=$XWIN_DIR/sdk/lib/um/x86_64 -Lnative=$XWIN_DIR/sdk/lib/ucrt/x86_64"

rustup target add x86_64-pc-windows-msvc

cd "$ROOT"
cargo build --release --target x86_64-pc-windows-msvc "$@"
echo "Built: $ROOT/target/x86_64-pc-windows-msvc/release/pausapatch.exe"
