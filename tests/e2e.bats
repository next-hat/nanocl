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
  cleanup_statefile ./tests/routing_recovery.yml
  local cargo
  for cargo in \
    test \
    e2e-multi-container \
    e2e-none \
    e2e-private-api \
    e2e-private-peer \
    e2e-default-client \
    e2e-routing-backend
  do
    nanocl cargo rm -yf "global.$cargo" >/dev/null 2>&1 || true
  done
  docker network rm e2e-private >/dev/null 2>&1 || true
  docker network rm e2e-routing-recovery >/dev/null 2>&1 || true
}

assert_dns_address() {
  local container="$1"
  local name="$2"
  local expected="$3"
  local lookup_args=("$name")
  if [ -n "${4:-}" ]; then
    lookup_args+=("$4")
  fi
  local answers
  local deadline=$((SECONDS + ${5:-60}))
  while [ "$SECONDS" -lt "$deadline" ]; do
    run timeout 5s docker exec "$container" nslookup "${lookup_args[@]}"
    answers=""
    if [ "$status" -eq 0 ]; then
      answers="$(printf '%s\n' "$output" | awk '
        /^Name:[[:space:]]/ { answer = 1; next }
        answer && /^Address(:|[[:space:]][0-9]+:)/ { print $NF }
      ')"
      if [ "$answers" = "$expected" ]; then
        return 0
      fi
    fi
    sleep 1
  done
  echo "expected ${name} to resolve to ${expected}, got ${answers:-<none>}" >&2
  return 1
}

assert_dns_missing() {
  local container="$1"
  local name="$2"
  run docker exec "$container" nslookup "$name"
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

wait_unix_health() {
  local socket="$1"
  local url="$2"
  local health_error=""
  local deadline=$((SECONDS + 120))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if health_error="$(curl --silent --show-error --fail --max-time 2 \
      --output /dev/null --unix-socket "$socket" "$url" 2>&1)"; then
      return 0
    fi
    sleep 1
  done
  echo "service did not recover: $socket $url: $health_error" >&2
  ls -l "$socket" >&2 || true
  return 1
}

ensure_containers_running() {
  local container running failed=0
  for container in "$@"; do
    if ! running="$(timeout 10s docker inspect \
      --format '{{.State.Running}}' "$container")"; then
      failed=1
      continue
    fi
    [ "$running" = true ] && continue

    # Docker's restart policy may win the race after the first inspection.
    # Keep start diagnostics, and accept only an observed running container.
    timeout 30s docker start "$container" || true
    if ! running="$(timeout 10s docker inspect \
      --format '{{.State.Running}}' "$container")" ||
      [ "$running" != true ]; then
      echo "container did not start: $container" >&2
      failed=1
    fi
  done
  return "$failed"
}

teardown() {
  local recovery_failed=0
  if [ -z "${BATS_TEST_COMPLETED:-}" ] && [ -n "${E2E_RECOVERY_PROXY:-}" ]; then
    local service
    for service in "$E2E_RECOVERY_DAEMON" "$E2E_RECOVERY_PROXY" "$E2E_RECOVERY_DNS"; do
      echo "Recovery service state before cleanup: $service" >&2
      timeout 10s docker inspect --format \
        '{{.Name}} status={{.State.Status}} exit_code={{.State.ExitCode}} error={{.State.Error}}' \
        "$service" >&2 || true
      timeout 10s docker logs --tail 80 "$service" >&2 || true
    done
  fi
  # The restart regression holds nanocld offline deliberately. Restore its
  # original policy and services even when an intermediate assertion fails.
  if [ -n "${E2E_RECOVERY_DAEMON_POLICY:-}" ]; then
    timeout 90s sudo -n systemctl start docker || recovery_failed=1
    timeout 20s docker update --restart="$E2E_RECOVERY_DAEMON_POLICY" \
      "$E2E_RECOVERY_DAEMON" || recovery_failed=1
    ensure_containers_running "$E2E_RECOVERY_PROXY" "$E2E_RECOVERY_DNS" \
      "$E2E_RECOVERY_DAEMON" || recovery_failed=1
    wait_unix_health /run/nanocl/nanocl.sock http://localhost/v0.0/version || recovery_failed=1
  fi
  if [ -z "${BATS_TEST_COMPLETED:-}" ] && [ -n "${E2E_DIAGNOSTIC_CARGO:-}" ]; then
    echo "Cargo container state before cleanup: $E2E_DIAGNOSTIC_CARGO" >&2
    local container_ids container_id
    container_ids="$(timeout 10s docker ps --all --no-trunc --quiet \
      --filter "label=io.nanocl.c=$E2E_DIAGNOSTIC_CARGO")" || true
    if [ -n "$container_ids" ]; then
      for container_id in $container_ids; do
        timeout 10s docker inspect --format \
          '{{.Id}} {{.Name}} status={{.State.Status}} exit_code={{.State.ExitCode}}' \
          "$container_id" >&2 || true
      done
    else
      echo "No matching Cargo containers remain" >&2
    fi
  fi
  cleanup_nanocl_artifacts
  [ "$recovery_failed" -eq 0 ]
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
  E2E_DIAGNOSTIC_CARGO="global.test"
  run nanocl cargo run test nginx:latest
  [ "$status" -eq 0 ]

  # Existing Cargo operations require a canonical namespace.name key.
  # A rejected batch item must fail the command and leave the Cargo intact.
  run nanocl cargo rm -yf test
  [ "$status" -ne 0 ]
  [[ "$output" == *"400 Bad Request"* ]]

  run nanocl cargo inspect global.test
  [ "$status" -eq 0 ]

  run nanocl cargo rm -yf global.test
  [ "$status" -eq 0 ]

  run nanocl cargo inspect global.test
  [ "$status" -ne 0 ]
  [[ "$output" == *"404 Not Found"* ]]
}

@test "nanocl applies, renders, serves, and removes a deployment Statefile" {
  E2E_DIAGNOSTIC_CARGO="global.deploy-example"
  run nanocl state apply -ys ./examples/deploy_example.yml
  [ "$status" -eq 0 ]

  run nanocl state render -s ./examples/deploy_example.yml
  [ "$status" -eq 0 ]

  run curl --silent --show-error --fail --retry 10 --retry-all-errors \
    --retry-delay 1 --retry-max-time 30 --connect-timeout 2 --max-time 5 \
    --header "Host: deploy-example.com" 127.0.0.1
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

  run docker inspect --format \
    '{{range .HostConfig.Dns}}{{println .}}{{end}}' "$peer_container"
  [ "$status" -eq 0 ]
  [ "$output" = "$gateway" ]

  run curl --silent --show-error --fail --retry 10 --retry-all-errors \
    --retry-delay 1 --retry-max-time 30 --connect-timeout 2 --max-time 5 \
    --header 'Host: network-partitioning.nanocl.test' "http://${gateway}"
  [ "$status" -eq 0 ]

  run docker exec "$peer_container" sh -c \
    'wget -q -T 5 -O - --header="Host: network-partitioning.nanocl.test" "http://${E2E_INTERNAL_GATEWAY}"'
  [ "$status" -eq 0 ]

  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"

  sleep 11
  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"

  run nanocl state apply -ys \
    ./tests/network_partitioning_dns_secondary.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"
  assert_dns_address \
    "$peer_container" network-partitioning-secondary.nanocl.test "$gateway"

  run nanocl state apply -ys \
    ./tests/network_partitioning_dns_secondary_updated.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"
  assert_dns_address \
    "$peer_container" network-partitioning-secondary-updated.nanocl.test \
    192.0.2.42
  assert_dns_missing \
    "$peer_container" network-partitioning-secondary.nanocl.test

  run nanocl state rm -ys ./tests/network_partitioning_dns_secondary.yml
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"
  assert_dns_missing \
    "$peer_container" network-partitioning-secondary-updated.nanocl.test

  run nanocl cargo restart system.ncdns
  [ "$status" -eq 0 ]
  assert_dns_address \
    "$peer_container" network-partitioning.nanocl.test "$gateway"

  run nanocl state rm -ys ./tests/network_partitioning.yml
  [ "$status" -eq 0 ]

  assert_no_cargo_containers global.e2e-private-api
  assert_no_cargo_containers global.e2e-private-peer
  assert_no_cargo_containers global.e2e-default-client

  run docker network inspect e2e-private
  [ "$status" -eq 0 ]
}

@test "ncproxy and ncdns recover changed addresses after Docker restarts" {
  # This restarts the host Docker service. CI opts in on its disposable runner;
  # local runs must explicitly opt in on a dedicated test host as well.
  [ "${NANOCL_E2E_DOCKER_RESTART:-0}" = "1" ] || \
    skip "set NANOCL_E2E_DOCKER_RESTART=1 on a dedicated Docker test host"
  run sudo -n systemctl is-active docker
  [ "$status" -eq 0 ]
  run docker info --format '{{.LiveRestoreEnabled}}'
  [ "$status" -eq 0 ]
  [ "$output" = "false" ]

  run docker network create --subnet 192.0.2.0/24 --gateway 192.0.2.1 \
    e2e-routing-recovery
  [ "$status" -eq 0 ]
  run nanocl state apply -ys ./tests/routing_recovery.yml
  [ "$status" -eq 0 ]
  E2E_DIAGNOSTIC_CARGO=global.e2e-routing-backend
  local backend old_address old_started_at new_started_at new_address
  backend="$(running_cargo_container global.e2e-routing-backend app backend)"
  old_address="$(docker inspect --format \
    '{{(index .NetworkSettings.Networks "e2e-routing-recovery").IPAddress}}' "$backend")"
  [ -n "$old_address" ]
  old_started_at="$(docker inspect --format '{{.State.StartedAt}}' "$backend")"
  run curl --silent --fail --retry 20 --retry-all-errors \
    --retry-delay 1 --retry-max-time 60 --connect-timeout 2 --max-time 5 \
    --header 'Host: routing-recovery.nanocl.test' http://127.0.0.1:18080
  [ "$status" -eq 0 ]
  [ "$output" = "routing-recovery-ok" ]
  assert_dns_address "$backend" routing-recovery.nanocl.test 192.0.2.1 192.0.2.1

  # Inspect and history must stay byte-for-byte equal: recovery is not a
  # resource mutation and must not add a revision or rewrite dynamic selectors.
  local resource view index=0
  local snapshots=()
  for resource in e2e-routing-proxy e2e-routing-dns; do
    for view in inspect histories; do
      run curl --silent --show-error --fail --max-time 5 \
        --unix-socket /run/nanocl/nanocl.sock \
        "http://localhost/v0.18.0/resources/${resource}/${view}"
      [ "$status" -eq 0 ]
      snapshots+=("$output")
    done
  done

  E2E_RECOVERY_DAEMON="$(running_installer_cargo system.ndaemon)"
  E2E_RECOVERY_PROXY="$(running_installer_cargo system.ncproxy)"
  E2E_RECOVERY_DNS="$(running_installer_cargo system.ncdns)"
  E2E_RECOVERY_DAEMON_POLICY="$(docker inspect --format \
    '{{.HostConfig.RestartPolicy.Name}}{{if eq .HostConfig.RestartPolicy.Name "on-failure"}}:{{.HostConfig.RestartPolicy.MaximumRetryCount}}{{end}}' \
    "$E2E_RECOVERY_DAEMON")"
  [ -n "$E2E_RECOVERY_DAEMON_POLICY" ]
  run docker update --restart=no "$E2E_RECOVERY_DAEMON"
  [ "$status" -eq 0 ]
  run timeout 30s docker stop --timeout 10 "$E2E_RECOVERY_DAEMON"
  [ "$status" -eq 0 ]
  run timeout 30s docker stop --timeout 10 "$E2E_RECOVERY_PROXY" "$E2E_RECOVERY_DNS"
  [ "$status" -eq 0 ]

  # Force different addresses instead of depending on Docker's allocation
  # order. Keep the same backend container and canonical Statefile resources.
  run docker network disconnect e2e-routing-recovery "$backend"
  [ "$status" -eq 0 ]
  run docker network rm e2e-routing-recovery
  [ "$status" -eq 0 ]
  run docker network create --subnet 198.51.100.0/24 --gateway 198.51.100.1 \
    e2e-routing-recovery
  [ "$status" -eq 0 ]
  run docker network connect --ip 198.51.100.2 e2e-routing-recovery "$backend"
  [ "$status" -eq 0 ]
  run timeout 90s sudo -n systemctl restart docker
  [ "$status" -eq 0 ]
  run ensure_containers_running "$backend" "$E2E_RECOVERY_PROXY" "$E2E_RECOVERY_DNS"
  [ "$status" -eq 0 ]
  run docker inspect --format '{{.State.Running}}' "$E2E_RECOVERY_DAEMON"
  [ "$status" -eq 0 ]
  [ "$output" = "false" ]
  run docker inspect --format '{{.State.StartedAt}}' "$backend"
  [ "$status" -eq 0 ]
  [ "$output" != "$old_started_at" ]
  new_started_at="$output"
  new_address="$(docker inspect --format \
    '{{(index .NetworkSettings.Networks "e2e-routing-recovery").IPAddress}}' "$backend")"
  [ "$new_address" = "198.51.100.2" ]
  [ "$new_address" != "$old_address" ]
  wait_unix_health /run/nanocl/proxy.sock http://localhost/health
  wait_unix_health /run/nanocl/dns.sock http://localhost/health

  # The backend start event has already happened while nanocld was offline.
  # Neither a Statefile reapply nor a workload restart may repair the routes.
  run ensure_containers_running "$E2E_RECOVERY_DAEMON"
  [ "$status" -eq 0 ]
  wait_unix_health /run/nanocl/nanocl.sock http://localhost/v0.0/version
  run timeout 100s curl --silent --fail --retry 90 --retry-all-errors \
    --retry-delay 1 --retry-max-time 90 --connect-timeout 2 --max-time 5 \
    --header 'Host: routing-recovery.nanocl.test' http://127.0.0.1:18080
  [ "$status" -eq 0 ]
  [ "$output" = "routing-recovery-ok" ]
  assert_dns_address "$backend" routing-recovery.nanocl.test 198.51.100.1 198.51.100.1 90

  # Controllers must also recover independently while nanocld stays available.
  # The backend remains untouched, so a Cargo lifecycle event cannot help.
  run timeout 40s docker restart --timeout 10 "$E2E_RECOVERY_PROXY" "$E2E_RECOVERY_DNS"
  [ "$status" -eq 0 ]
  wait_unix_health /run/nanocl/proxy.sock http://localhost/health
  wait_unix_health /run/nanocl/dns.sock http://localhost/health
  run timeout 100s curl --silent --fail --retry 90 --retry-all-errors \
    --retry-delay 1 --retry-max-time 90 --connect-timeout 2 --max-time 5 \
    --header 'Host: routing-recovery.nanocl.test' http://127.0.0.1:18080
  [ "$status" -eq 0 ]
  [ "$output" = "routing-recovery-ok" ]
  assert_dns_address "$backend" routing-recovery.nanocl.test 198.51.100.1 198.51.100.1 90

  # Change only the endpoint while the daemon and its event stream stay alive.
  # Network connect/disconnect does not refresh the process observation, so the
  # periodic inventory must repair this address without a Cargo start event.
  local daemon_started_at
  daemon_started_at="$(docker inspect --format '{{.State.StartedAt}}' "$E2E_RECOVERY_DAEMON")"
  run docker network disconnect e2e-routing-recovery "$backend"
  [ "$status" -eq 0 ]
  run docker network connect --ip 198.51.100.3 e2e-routing-recovery "$backend"
  [ "$status" -eq 0 ]
  run docker inspect --format \
    '{{(index .NetworkSettings.Networks "e2e-routing-recovery").IPAddress}}' "$backend"
  [ "$status" -eq 0 ]
  [ "$output" = "198.51.100.3" ]
  run timeout 370s curl --silent --fail --retry 360 --retry-all-errors \
    --retry-delay 1 --retry-max-time 360 --connect-timeout 2 --max-time 5 \
    --header 'Host: routing-recovery.nanocl.test' http://127.0.0.1:18080
  [ "$status" -eq 0 ]
  [ "$output" = "routing-recovery-ok" ]
  run docker inspect --format '{{.State.StartedAt}}' "$E2E_RECOVERY_DAEMON"
  [ "$status" -eq 0 ]
  [ "$output" = "$daemon_started_at" ]
  run docker inspect --format '{{.State.StartedAt}}' "$backend"
  [ "$status" -eq 0 ]
  [ "$output" = "$new_started_at" ]
  [ "$(running_cargo_container global.e2e-routing-backend app backend)" = "$backend" ]
  for resource in e2e-routing-proxy e2e-routing-dns; do
    for view in inspect histories; do
      run curl --silent --show-error --fail --max-time 5 \
        --unix-socket /run/nanocl/nanocl.sock \
        "http://localhost/v0.18.0/resources/${resource}/${view}"
      [ "$status" -eq 0 ]
      [ "$output" = "${snapshots[$index]}" ]
      index=$((index + 1))
    done
  done
}

@test "nanocl rejects an invalid Statefile" {
  run nanocl state apply -ys ./tests/invalid_statefile.yaml
  [ "$status" -ne 0 ]
}
