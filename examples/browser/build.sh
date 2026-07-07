#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

WASM="${1:-$PROJECT_DIR/target/wasm32-wasip2/release/phrust-php.wasm}"

if [ ! -f "$WASM" ]; then
  echo "Error: WASM file not found at $WASM" >&2
  echo "Build it first:" >&2
  echo "  CC_wasm32_wasip2=... CFLAGS='-U SUPPORT_JIT' PCRE2_SYS_STATIC=1 \\" >&2
  echo "    cargo build --release --target wasm32-wasip2 -p php_vm_cli" >&2
  exit 1
fi

echo "=== jco transpile ==="
jco transpile "$WASM" -o "$SCRIPT_DIR"

echo "=== create worker copy with resolved import-map specifiers ==="
sed \
  -e "s|from '@bytecodealliance/preview2-shim/cli'|from './node_modules/@bytecodealliance/preview2-shim/dist/browser/cli.js'|g" \
  -e "s|from '@bytecodealliance/preview2-shim/clocks'|from './node_modules/@bytecodealliance/preview2-shim/dist/browser/clocks.js'|g" \
  -e "s|from '@bytecodealliance/preview2-shim/filesystem'|from './node_modules/@bytecodealliance/preview2-shim/dist/browser/filesystem.js'|g" \
  -e "s|from '@bytecodealliance/preview2-shim/io'|from './shim-wrappers/io.js'|g" \
  -e "s|from '@bytecodealliance/preview2-shim/random'|from './node_modules/@bytecodealliance/preview2-shim/dist/browser/random.js'|g" \
  -e "s|from '@bytecodealliance/preview2-shim/sockets'|from './shim-wrappers/sockets.js'|g" \
  "$SCRIPT_DIR/phrust-php.js" > "$SCRIPT_DIR/phrust-php-worker.js"

echo "=== copy worker entry ==="
cp "$SCRIPT_DIR/phrust-worker.mjs" "$SCRIPT_DIR/phrust-worker.js"

echo "Serve with: target/release/phrust-server --docroot $SCRIPT_DIR --listen 127.0.0.1:8080"