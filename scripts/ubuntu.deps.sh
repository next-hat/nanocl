#!/bin/sh -i
## name: ubuntu.deps.sh
set -e -x

sudo apt install -y pkg-config gcc musl-dev libpq-dev libssl-dev musl-tools libc-dev g++ make
