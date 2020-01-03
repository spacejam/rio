#!/bin/bash
set -eo pipefail

echo "asan"
cargo clean
export RUSTFLAGS="-Z sanitizer=address"
export ASAN_OPTIONS="detect_odr_violation=0"
cargo +nightly test --target x86_64-unknown-linux-gnu
unset ASAN_OPTIONS

echo "lsan"
cargo clean
export RUSTFLAGS="-Z sanitizer=leak"
cargo +nightly run --example=o_direct --target x86_64-unknown-linux-gnu

echo "tsan"
cargo clean
export RUSTFLAGS="-Z sanitizer=thread"
cargo +nightly run --example=o_direct --target x86_64-unknown-linux-gnu
