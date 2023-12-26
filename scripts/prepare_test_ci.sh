docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  cargo build --no-default-features --features "dev" --bin nanocld

docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  cargo build --no-default-features --features "dev" --bin ncproxy

docker run -d --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev \
  cargo run --no-default-features --features "dev" --bin nanocld -- --hosts tcp://0.0.0.0:8585 --state-dir $HOME/.nanocl_dev/state

docker run -d --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev \
  cargo run --no-default-features --features "dev" --bin ncproxy -- --state-dir $HOME/.nanocl_dev/state
