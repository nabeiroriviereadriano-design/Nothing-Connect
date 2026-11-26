#!/usr/bin/env bash
# Compile le cœur Rust pour Android et génère les liaisons Kotlin (UniFFI).
# Prérequis : NDK (ANDROID_NDK_HOME), cibles rustup Android, cargo-ndk.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/apps/android/app/src/main"
PROFILE="${1:-release}"

cd "$ROOT"
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -P 29 -o "$APP/jniLibs" build -p nc-ffi --profile "$PROFILE"

# Liaisons Kotlin générées depuis la bibliothèque hôte (même version d'UniFFI).
cargo build -p nc-ffi
cargo run -q -p nc-ffi --bin uniffi-bindgen -- generate \
  --library "target/debug/libnc_ffi.so" --language kotlin --out-dir "$APP/java" --no-format
echo "OK : bibliothèques dans $APP/jniLibs, liaisons dans $APP/java/uniffi"
