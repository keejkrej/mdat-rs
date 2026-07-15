#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack is required. Install with: cargo install wasm-pack" >&2
  exit 1
fi

if command -v brew >/dev/null 2>&1 && brew --prefix llvm >/dev/null 2>&1; then
  LLVM="$(brew --prefix llvm)"
  export CC_wasm32_unknown_unknown="${LLVM}/bin/clang"
  export AR_wasm32_unknown_unknown="${LLVM}/bin/llvm-ar"
fi

if command -v brew >/dev/null 2>&1 && brew --prefix lld >/dev/null 2>&1; then
  LLD="$(brew --prefix lld)"
  export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="${LLD}/bin/wasm-ld"
fi

wasm-pack build crates/mdat-wasm --target web --out-dir pkg
