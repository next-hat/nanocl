#!/bin/sh
## name: runner.sh

docker run -it --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v rust-cache:/usr/local/cargo/registry \
  -v nanocl-deps:/project/target \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev "$@"
