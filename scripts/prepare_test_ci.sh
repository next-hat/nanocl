#!/bin/sh

set -eu

docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  build --bin nanocld --no-default-features --features "test"

docker run -i --rm \
  -v $(pwd):/project \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  ghcr.io/next-hat/nanocl-dev:dev \
  build --bin ncproxy --no-default-features --features "dev"

# Wait for CockroachDB SQL endpoint before starting nanocld.
i=0
while [ "$i" -lt 120 ]; do
  if docker exec nstore.system.c \
    cockroach sql --insecure --host=127.0.0.1:26258 -e "select 1" \
    >/dev/null 2>&1; then
    break
  fi
  i=$((i + 1))
  sleep 1
done

if [ "$i" -eq 120 ]; then
  echo "nstore did not become ready" >&2
  docker logs nstore.system.c || true
  exit 1
fi

docker rm -f nanocld-ci ncproxy-ci >/dev/null 2>&1 || true

docker run -d \
  --name nanocld-ci \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -v /run/nanocl:/run/nanocl \
  -e HOME=$HOME \
  -w /project \
  --hostname nanocl.internal \
  --network host \
  --add-host store.nanocl.internal:127.0.0.1 \
  --add-host nanocl.internal:127.0.0.1 \
  ghcr.io/next-hat/nanocl-dev:dev \
  run --bin nanocld --no-default-features --features "test" -- --store-addr postgresql://root:root@store.nanocl.internal:26258/defaultdb\
    --hosts tcp://0.0.0.0:8585 --state-dir $HOME/.nanocl_dev/state

docker run -d \
  --name ncproxy-ci \
  -v $(pwd):/project \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $HOME/.cargo/registry:/usr/local/cargo/registry \
  -v $HOME/.nanocl_dev/state:/$HOME/.nanocl_dev/state \
  -v /tmp:/tmp \
  -v /run/nanocl:/run/nanocl \
  -e HOME=$HOME \
  -w /project \
  --network host \
  --add-host nanocl.internal:127.0.0.1 \
  ghcr.io/next-hat/nanocl-dev:dev \
  run --bin ncproxy --no-default-features --features "dev" -- --state-dir $HOME/.nanocl_dev/state/proxy

API_BASE_URL="http://127.0.0.1:8585/v0.0"

i=0
while [ "$i" -lt 180 ]; do
  if curl --silent --fail "$API_BASE_URL/resource/kinds/ncproxy.io/rule/inspect" >/dev/null 2>&1; then
    exit 0
  fi

  if ! docker ps --format '{{.Names}}' | grep -q '^nanocld-ci$'; then
    echo "nanocld-ci exited unexpectedly" >&2
    docker logs nanocld-ci || true
    exit 1
  fi

  if ! docker ps --format '{{.Names}}' | grep -q '^ncproxy-ci$'; then
    echo "ncproxy-ci exited unexpectedly" >&2
    docker logs ncproxy-ci || true
    exit 1
  fi

  i=$((i + 1))
  sleep 1
done

echo "ncproxy resource kind did not become ready" >&2
docker logs nanocld-ci || true
docker logs ncproxy-ci || true
exit 1
