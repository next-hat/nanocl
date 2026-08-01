#!/usr/bin/env bats

cleanup_statefile() {
  local statefile="$1"
  nanocl state rm -ys "$statefile" >/dev/null 2>&1 || true
}

cleanup_nanocl_artifacts() {
  cleanup_statefile ./examples/deploy_example.yml
  cleanup_statefile ./examples/job_example.yml
  cleanup_statefile ./tests/job_with_error.yml
  cleanup_statefile ./tests/network_partitioning.yml
  nanocl cargo rm -yf test >/dev/null 2>&1 || true
  docker network rm e2e-private >/dev/null 2>&1 || true
}

setup_file() {
  cleanup_nanocl_artifacts
}

teardown() {
  cleanup_nanocl_artifacts
}

teardown_file() {
  cleanup_nanocl_artifacts
}

@test "nanocl --version" {
  run nanocl --version
  [ "$status" -eq 0 ]
}

@test "nanocl version" {
  run nanocl version
  [ "$status" -eq 0 ]
}

@test "nanocl help" {
  run nanocl help
  [ "$status" -eq 0 ]
}

@test "nanocl info" {
  run nanocl info
  [ "$status" -eq 0 ]
}

@test "nanocl cargo ls" {
  run nanocl cargo ls
  [ "$status" -eq 0 ]
}

@test "nanocl cargo run" {
  run nanocl cargo run test nginx:latest
  [ "$status" -eq 0 ]
}

@test "nanocl cargo rm" {
  run nanocl cargo rm -yf test
  [ "$status" -eq 0 ]
}

@test "nanocl state apply -ys ./examples/deploy_example.yml" {
  run nanocl state apply -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state render -s ./examples/deploy_example.yml" {
  run nanocl state render -s ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "curl --header \"Host: deploy-example.com\" 127.0.0.1" {
  run sleep 2
  run curl --header "Host: deploy-example.com" 127.0.0.1
  [ "$status" -eq 0 ]
}

@test "nanocl state rm -ys ./examples/deploy_example.yml" {
  run nanocl state rm -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state apply -ys ./examples/job_example.yml" {
  run nanocl state apply -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state rm -ys ./examples/job_example.yml" {
  run nanocl state rm -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl state apply -ys ./tests/network_partitioning.yml" {
  run docker network inspect e2e-private
  [ "$status" -ne 0 ]

  run nanocl state apply -ys ./tests/network_partitioning.yml
  [ "$status" -eq 0 ]

  run docker network inspect e2e-private
  [ "$status" -eq 0 ]

  api_container="$(docker ps -q --filter label=io.nanocl.c=global.e2e-private-api | head -n 1)"
  peer_container="$(docker ps -q --filter label=io.nanocl.c=global.e2e-private-peer | head -n 1)"
  default_container="$(docker ps -q --filter label=io.nanocl.c=global.e2e-default-client | head -n 1)"
  job_container="$(docker ps -aq --filter label=io.nanocl.j=e2e-private-job | head -n 1)"
  [ -n "$api_container" ]
  [ -n "$peer_container" ]
  [ -n "$default_container" ]
  [ -n "$job_container" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$api_container"
  [ "$status" -eq 0 ]
  [ "$output" = "e2e-private" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$job_container"
  [ "$status" -eq 0 ]
  [ "$output" = "e2e-private" ]

  run docker inspect --format \
    '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}} {{end}}' \
    "$api_container"
  [ "$status" -eq 0 ]
  [[ "$output" == *"e2e-private"* ]]
  [[ "$output" != *"nanoclbr0"* ]]

  run docker inspect --format \
    '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}} {{end}}' \
    "$default_container"
  [ "$status" -eq 0 ]
  [[ "$output" == *"nanoclbr0"* ]]
  [[ "$output" != *"e2e-private"* ]]

  api_address="$(docker inspect --format \
    '{{(index .NetworkSettings.Networks "e2e-private").IPAddress}}' \
    "$api_container")"
  [ -n "$api_address" ]

  run docker exec "$peer_container" \
    wget -q -T 5 -O - "http://${api_address}:9000"
  [ "$status" -eq 0 ]

  run docker exec "$default_container" \
    wget -q -T 2 -O - "http://${api_address}:9000"
  [ "$status" -ne 0 ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$peer_container"
  [ "$status" -eq 0 ]
  [ "$output" = "e2e-private" ]

  run curl --silent --fail --unix-socket /run/nanocl/nanocl.sock \
    http://localhost/v0.18.0/networks
  [ "$status" -eq 0 ]
  [[ "$output" == *'"Name":"e2e-private"'* ]]

  node_name="$(docker inspect --format \
    '{{range .Config.Env}}{{println .}}{{end}}' system.ncproxy.c | \
    sed -n 's/^NANOCL_NODE=//p' | head -n 1)"
  [ -n "$node_name" ]

  run curl --silent --fail --unix-socket /run/nanocl/nanocl.sock \
    "http://localhost/v0.18.0/networks/${node_name}.e2e-private/inspect"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"Key":"'"${node_name}"'.e2e-private"'* ]]

  gateway="$(docker network inspect --format '{{(index .IPAM.Config 0).Gateway}}' e2e-private)"
  [ -n "$gateway" ]

  run curl --silent --fail --retry 10 --retry-all-errors \
    --header 'Host: network-partitioning.nanocl.test' "http://${gateway}"
  [ "$status" -eq 0 ]

  run docker exec system.ncdns.c sh -c \
    "nslookup network-partitioning.nanocl.test '$gateway' | tail -n 1"
  [ "$status" -eq 0 ]

  run nanocl state rm -ys ./tests/network_partitioning.yml
  [ "$status" -eq 0 ]

  run docker network inspect e2e-private
  [ "$status" -eq 0 ]
}

@test "nanocl state apply fails with invalid statefile" {
  run nanocl state apply -ys ./tests/invalid_statefile.yaml
  [ "$status" -ne 0 ]
}
