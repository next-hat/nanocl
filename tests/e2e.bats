#!/usr/bin/env bats

cleanup_statefile() {
  local statefile="$1"
  nanocl state rm -ys "$statefile" >/dev/null 2>&1 || true
}

cleanup_nanocl_artifacts() {
  cleanup_statefile ./examples/deploy_example.yml
  cleanup_statefile ./examples/job_example.yml
  cleanup_statefile ./tests/job_with_error.yml
  cleanup_statefile ./tests/multi_container_networking.yml
  cleanup_statefile ./tests/cargo_none.yml
  cleanup_statefile ./tests/network_partitioning_dns_secondary.yml
  cleanup_statefile ./tests/network_partitioning.yml
  local cargo
  for cargo in \
    test \
    e2e-multi-container \
    e2e-none \
    e2e-private-api \
    e2e-private-peer \
    e2e-default-client
  do
    nanocl cargo rm -yf "$cargo" >/dev/null 2>&1 || true
  done
  docker network rm e2e-private >/dev/null 2>&1 || true
}

assert_dns_address() {
  local container="$1"
  local name="$2"
  local server="$3"
  local expected="$4"
  local answers
  run docker exec "$container" nslookup "$name" "$server"
  [ "$status" -eq 0 ]
  answers="$(printf '%s\n' "$output" | awk '
    /^Name:[[:space:]]/ { answer = 1; next }
    answer && /^Address(:|[[:space:]][0-9]+:)/ { print $NF }
  ')"
  [ "$answers" = "$expected" ]
}

assert_dns_missing() {
  local container="$1"
  local name="$2"
  local server="$3"
  run docker exec "$container" nslookup "$name" "$server"
  [ "$status" -ne 0 ]
}

running_cargo_container() {
  local cargo_key="$1"
  local role="$2"
  local logical_name="${3:-}"
  local ids
  local count
  local filters=(
    --filter "label=io.nanocl.c=${cargo_key}"
    --filter "label=io.nanocl.cargo.role=${role}"
  )
  if [ -n "$logical_name" ]; then
    filters+=(--filter "label=io.nanocl.cargo.container=${logical_name}")
  fi
  ids="$(docker ps --no-trunc --quiet "${filters[@]}")" || return 1
  count="$(printf '%s\n' "$ids" | awk 'NF { count++ } END { print count + 0 }')"
  if [ "$count" -ne 1 ]; then
    echo "expected one running ${role} container ${logical_name:-<any>} for ${cargo_key}, found ${count}" >&2
    docker ps --no-trunc "${filters[@]}" >&2
    return 1
  fi
  printf '%s\n' "$ids"
}

running_installer_cargo() {
  local cargo_key="$1"
  local ids
  local count
  ids="$(docker ps --no-trunc --quiet \
    --filter "label=io.nanocl.c=${cargo_key}" \
    --filter label=io.nanocl.not-init-c=true)" || return 1
  count="$(printf '%s\n' "$ids" | awk 'NF { count++ } END { print count + 0 }')"
  if [ "$count" -ne 1 ]; then
    echo "expected one running installer Cargo container for ${cargo_key}, found ${count}" >&2
    docker ps --no-trunc \
      --filter "label=io.nanocl.c=${cargo_key}" >&2
    return 1
  fi
  printf '%s\n' "$ids"
}

assert_no_cargo_containers() {
  local cargo_key="$1"
  local ids
  local attempt
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    ids="$(docker ps --all --quiet \
      --filter "label=io.nanocl.c=${cargo_key}")"
    if [ -z "$ids" ]; then
      return 0
    fi
    sleep 1
  done
  docker ps --all --no-trunc \
    --filter "label=io.nanocl.c=${cargo_key}" >&2
  return 1
}

assert_no_cargo_role_containers() {
  local cargo_key="$1"
  local role="$2"
  local ids
  ids="$(docker ps --all --quiet \
    --filter "label=io.nanocl.c=${cargo_key}" \
    --filter "label=io.nanocl.cargo.role=${role}")" || return 1
  if [ -z "$ids" ]; then
    return 0
  fi
  docker ps --all --no-trunc \
    --filter "label=io.nanocl.c=${cargo_key}" \
    --filter "label=io.nanocl.cargo.role=${role}" >&2
  return 1
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

@test "nanocl prints its CLI version" {
  run nanocl --version
  [ "$status" -eq 0 ]
}

@test "nanocl prints CLI and daemon versions" {
  run nanocl version
  [ "$status" -eq 0 ]
}

@test "nanocl displays help" {
  run nanocl help
  [ "$status" -eq 0 ]
}

@test "nanocl displays host information" {
  run nanocl info
  [ "$status" -eq 0 ]
}

@test "nanocl lists Cargo resources" {
  run nanocl cargo ls
  [ "$status" -eq 0 ]
}

@test "nanocl runs and removes a Cargo" {
  run nanocl cargo run test nginx:latest
  [ "$status" -eq 0 ]

  run nanocl cargo rm -yf test
  [ "$status" -eq 0 ]
}

@test "nanocl applies, renders, serves, and removes a deployment Statefile" {
  run nanocl state apply -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]

  run nanocl state render -s ./examples/deploy_example.yml
  [ "$status" -eq 0 ]

  run sleep 2
  run curl --header "Host: deploy-example.com" 127.0.0.1
  [ "$status" -eq 0 ]

  run nanocl state rm -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl applies and removes a Job Statefile" {
  run nanocl state apply -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]

  run nanocl state rm -ys ./examples/job_example.yml
  [ "$status" -eq 0 ]
}

@test "nanocl runs one Cargo replica with shared default networking and published ports" {
  local cargo_key="global.e2e-multi-container"

  run nanocl state apply -ys ./tests/multi_container_networking.yml
  [ "$status" -eq 0 ]

  sandbox_id="$(running_cargo_container "$cargo_key" sandbox _sandbox)"
  server_id="$(running_cargo_container "$cargo_key" app server)"
  client_id="$(running_cargo_container "$cargo_key" app client)"

  app_ids="$(docker ps --no-trunc --quiet \
    --filter "label=io.nanocl.c=${cargo_key}" \
    --filter label=io.nanocl.cargo.role=app)"
  app_count="$(printf '%s\n' "$app_ids" | awk 'NF { count++ } END { print count + 0 }')"
  [ "$app_count" -eq 2 ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$server_id"
  [ "$status" -eq 0 ]
  [ "$output" = "container:${sandbox_id}" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$client_id"
  [ "$status" -eq 0 ]
  [ "$output" = "container:${sandbox_id}" ]

  server_namespace="$(docker exec "$server_id" readlink /proc/self/ns/net)"
  client_namespace="$(docker exec "$client_id" readlink /proc/self/ns/net)"
  [ -n "$server_namespace" ]
  [ "$server_namespace" = "$client_namespace" ]

  server_ip="$(docker exec "$server_id" hostname -i)"
  client_ip="$(docker exec "$client_id" hostname -i)"
  [ -n "$server_ip" ]
  [ "$server_ip" = "$client_ip" ]

  run docker exec "$client_id" sh -c \
    'for attempt in 1 2 3 4 5; do wget -q -T 2 -O - http://127.0.0.1:9000 && exit 0; sleep 1; done; exit 1'
  [ "$status" -eq 0 ]
  [[ "$output" == *"multi-container-ok"* ]]

  run docker inspect --format \
    '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}} {{end}}' \
    "$sandbox_id"
  [ "$status" -eq 0 ]
  [[ "$output" == *"nanoclbr0"* ]]

  run docker inspect --format \
    '{{if .HostConfig.PortBindings}}present{{else}}absent{{end}}' \
    "$sandbox_id"
  [ "$status" -eq 0 ]
  [ "$output" = "present" ]

  run docker inspect --format \
    '{{if .HostConfig.PortBindings}}present{{else}}absent{{end}}' \
    "$server_id"
  [ "$status" -eq 0 ]
  [ "$output" = "absent" ]

  host_port="$(docker inspect --format \
    '{{(index (index .NetworkSettings.Ports "9000/tcp") 0).HostPort}}' \
    "$sandbox_id")"
  [[ "$host_port" =~ ^[0-9]+$ ]]

  run curl --silent --fail --retry 10 --retry-all-errors \
    "http://127.0.0.1:${host_port}"
  [ "$status" -eq 0 ]
  [[ "$output" == *"multi-container-ok"* ]]

  run nanocl state rm -ys ./tests/multi_container_networking.yml
  [ "$status" -eq 0 ]
  assert_no_cargo_containers "$cargo_key"
}

@test "nanocl runs a Cargo in none mode with shared localhost and no external network" {
  local cargo_key="global.e2e-none"

  run nanocl state apply -ys ./tests/cargo_none.yml
  [ "$status" -eq 0 ]

  sandbox_id="$(running_cargo_container "$cargo_key" sandbox _sandbox)"
  server_id="$(running_cargo_container "$cargo_key" app server)"
  client_id="$(running_cargo_container "$cargo_key" app client)"

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$sandbox_id"
  [ "$status" -eq 0 ]
  [ "$output" = "none" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$server_id"
  [ "$status" -eq 0 ]
  [ "$output" = "container:${sandbox_id}" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$client_id"
  [ "$status" -eq 0 ]
  [ "$output" = "container:${sandbox_id}" ]

  run docker inspect --format \
    '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}} {{end}}' \
    "$sandbox_id"
  [ "$status" -eq 0 ]
  [[ "$output" != *"nanoclbr0"* ]]
  [[ "$output" != *"e2e-private"* ]]
  [[ "$output" != *"bridge"* ]]

  run docker exec "$client_id" sh -c \
    'for attempt in 1 2 3 4 5; do wget -q -T 2 -O - http://127.0.0.1:9000 && exit 0; sleep 1; done; exit 1'
  [ "$status" -eq 0 ]
  [[ "$output" == *"cargo-none-ok"* ]]

  run nanocl state rm -ys ./tests/cargo_none.yml
  [ "$status" -eq 0 ]
  assert_no_cargo_containers "$cargo_key"
}

@test "nanocl isolates and routes workloads on a custom network" {
  run docker network inspect e2e-private
  [ "$status" -ne 0 ]

  run nanocl state apply -ys ./tests/network_partitioning.yml
  [ "$status" -eq 0 ]

  run docker network inspect e2e-private
  [ "$status" -eq 0 ]

  api_container="$(running_cargo_container global.e2e-private-api app api)"
  peer_container="$(running_cargo_container global.e2e-private-peer app peer)"
  default_container="$(running_cargo_container global.e2e-default-client app client)"
  assert_no_cargo_role_containers global.e2e-private-api sandbox
  assert_no_cargo_role_containers global.e2e-private-peer sandbox
  assert_no_cargo_role_containers global.e2e-default-client sandbox
  job_container="$(docker ps -aq --filter label=io.nanocl.j=e2e-private-job | head -n 1)"
  [ -n "$job_container" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$api_container"
  [ "$status" -eq 0 ]
  [ "$output" = "e2e-private" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$peer_container"
  [ "$status" -eq 0 ]
  [ "$output" = "e2e-private" ]

  run docker inspect --format '{{.HostConfig.NetworkMode}}' "$default_container"
  [ "$status" -eq 0 ]
  [ "$output" = "nanoclbr0" ]

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

  run docker exec "$peer_container" sh -c \
    "for attempt in 1 2 3 4 5; do wget -q -T 2 -O - 'http://${api_address}:9000' && exit 0; sleep 1; done; exit 1"
  [ "$status" -eq 0 ]

  run docker exec "$default_container" \
    wget -q -T 2 -O - "http://${api_address}:9000"
  [ "$status" -ne 0 ]

  run curl --silent --fail --unix-socket /run/nanocl/nanocl.sock \
    http://localhost/v0.18.0/networks
  [ "$status" -eq 0 ]
  [[ "$output" == *'"Name":"e2e-private"'* ]]

  ncproxy_container="$(running_installer_cargo system.ncproxy)"
  node_name="$(docker inspect --format \
    '{{range .Config.Env}}{{println .}}{{end}}' "$ncproxy_container" | \
    sed -n 's/^NANOCL_NODE=//p' | head -n 1)"
  [ -n "$node_name" ]

  run curl --silent --fail --unix-socket /run/nanocl/nanocl.sock \
    "http://localhost/v0.18.0/networks/${node_name}.e2e-private/inspect"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"Key":"'"${node_name}"'.e2e-private"'* ]]

  gateway="$(docker network inspect --format '{{(index .IPAM.Config 0).Gateway}}' e2e-private)"
  [ -n "$gateway" ]

  compiled_gateway="$(docker inspect --format \
    '{{range .Config.Env}}{{println .}}{{end}}' "$peer_container" | \
    sed -n 's/^E2E_INTERNAL_GATEWAY=//p')"
  [ "$compiled_gateway" = "$gateway" ]

  run curl --silent --fail --retry 10 --retry-all-errors \
    --header 'Host: network-partitioning.nanocl.test' "http://${gateway}"
  [ "$status" -eq 0 ]

  run docker exec "$peer_container" sh -c \
    'wget -q -T 5 -O - --header="Host: network-partitioning.nanocl.test" "http://${E2E_INTERNAL_GATEWAY}"'
  [ "$status" -eq 0 ]

  ncdns_container="$(running_installer_cargo system.ncdns)"
  assert_dns_address \
    "$ncdns_container" network-partitioning.nanocl.test "$gateway" "$gateway"

  sleep 11
  assert_dns_address \
    "$ncdns_container" network-partitioning.nanocl.test "$gateway" "$gateway"

  run nanocl state apply -ys \
    ./tests/network_partitioning_dns_secondary.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$ncdns_container" network-partitioning.nanocl.test "$gateway" "$gateway"
  assert_dns_address \
    "$ncdns_container" network-partitioning-secondary.nanocl.test \
    "$gateway" "$gateway"

  run nanocl state apply -ys \
    ./tests/network_partitioning_dns_secondary_updated.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$ncdns_container" network-partitioning.nanocl.test "$gateway" "$gateway"
  assert_dns_address \
    "$ncdns_container" network-partitioning-secondary.nanocl.test \
    "$gateway" 192.0.2.42

  run nanocl state rm -ys ./tests/network_partitioning_dns_secondary.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$ncdns_container" network-partitioning.nanocl.test "$gateway" "$gateway"
  assert_dns_missing \
    "$ncdns_container" network-partitioning-secondary.nanocl.test "$gateway"

  run nanocl state rm -ys ./tests/network_partitioning.yml
  [ "$status" -eq 0 ]

  assert_no_cargo_containers global.e2e-private-api
  assert_no_cargo_containers global.e2e-private-peer
  assert_no_cargo_containers global.e2e-default-client

  run docker network inspect e2e-private
  [ "$status" -eq 0 ]
}

@test "nanocl rejects an invalid Statefile" {
  run nanocl state apply -ys ./tests/invalid_statefile.yaml
  [ "$status" -ne 0 ]
}
