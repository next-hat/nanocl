#!/bin/sh -i
## name: rust.deps.sh
set -e -x

cargo install cargo-make
cargo install cargo-nextest
cargo install cargo-llvm-cov
