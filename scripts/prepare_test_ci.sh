docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  build --bin nanocld --no-default-features --features "dev"

docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  build --bin ncproxy --no-default-features --features "dev"

docker run -d --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -v /run/nanocl:/run/nanocl \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev \
  run --bin nanocld --no-default-features --features "dev" -- --hosts tcp://0.0.0.0:8585 --state-dir $HOME/.nanocl_dev/state

docker run -d --rm \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -v /run/nanocl:/run/nanocl \
  -e HOME=$HOME \
  -w /project \
  --network host \
  ghcr.io/next-hat/nanocl-dev:dev \
  run --bin ncproxy --no-default-features --features "dev" -- --state-dir $HOME/.nanocl_dev/state/proxy
