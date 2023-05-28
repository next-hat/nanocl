#!/bin/sh -i
## name: rust.deps.sh
set -e -x

rustup component add llvm-tools-preview --toolchain stable-x86_64-unknown-linux-gnu
cargo install cargo-make
cargo install cargo-nextest
cargo install cargo-llvm-cov
