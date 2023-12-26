#!/bin/sh
## name: runnerci.sh

docker run -i --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev "$@"
