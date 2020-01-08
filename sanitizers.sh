#!/bin/bash
set -eo pipefail

echo "asan"
cargo clean
export RUSTFLAGS="-Z sanitizer=address"
export ASAN_OPTIONS="detect_odr_violation=0"
cargo +nightly build --example nop --target x86_64-unknown-linux-gnu
./target/debug/examples/nop
unset ASAN_OPTIONS

echo "lsan"
cargo clean
export RUSTFLAGS="-Z sanitizer=leak"
cargo +nightly build --example nop --target x86_64-unknown-linux-gnu
./target/debug/examples/nop

echo "tsan"
cargo clean
export RUSTFLAGS="-Z sanitizer=thread"
cargo +nightly build --example nop --target x86_64-unknown-linux-gnu
./target/debug/examples/nop
