#!/usr/bin/env bash
# Prepare a local/CI environment for E2E tests.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

NANOCL_CHANNEL="${NANOCL_CHANNEL:-nightly}"
E2E_RUNNER_USER="${E2E_RUNNER_USER:-${USER:-$(id -un)}}"
export E2E_RUNNER_USER

system_app_id() {
  local name="$1"
  local ids
  local count
  ids="$(docker ps --no-trunc --quiet \
    --filter "label=io.nanocl.c=system.${name}" \
    --filter label=io.nanocl.not-init-c=true)"
  count="$(printf '%s\n' "$ids" | awk 'NF { count++ } END { print count + 0 }')"
  [ "$count" -eq 1 ] || return 1
  printf '%s\n' "$ids"
}

log_system_app() {
  local name="$1"
  local id
  id="$(docker ps --all --no-trunc --quiet \
    --filter "label=io.nanocl.c=system.${name}" \
    --filter label=io.nanocl.not-init-c=true | head -n 1)"
  if [ -n "$id" ]; then
    docker logs "$id" || true
  else
    echo "No system.${name} application container found" >&2
  fi
}

unix_health_status() {
  local socket_path="$1"
  local url="$2"
  local status

  status="$(curl --silent --output /dev/null --write-out "%{http_code}" \
    --unix-socket "$socket_path" "$url" 2>/dev/null || true)"
  printf '%s\n' "${status:-000}"
}

diagnose_unix_service() {
  local name="$1"
  local socket_path="$2"
  local id

  ls -la /run/nanocl || true
  stat "$socket_path" || true
  id="$(system_app_id "$name" || true)"
  if [ -n "$id" ]; then
    docker exec "$id" sh -c \
      'id; ls -la /run/nanocl; grep "$1" /proc/net/unix || true' \
      sh "$socket_path" || true
    docker inspect "$id" \
      --format '{{range .Mounts}}{{println .Source "->" .Destination}}{{end}}' \
      || true
  fi
  curl --show-error --verbose --unix-socket "$socket_path" \
    http://localhost/health || true
}

if ! getent group nanocl >/dev/null 2>&1; then
  sudo groupadd nanocl
fi
sudo usermod -aG nanocl "$E2E_RUNNER_USER"

e2e_nanocl_gid="$(getent group nanocl | awk -F: '{ print $3 }')"
if [[ " $(id -G) " != *" ${e2e_nanocl_gid} "* ]]; then
  if [ "${E2E_NANOCL_GROUP_REEXEC:-0}" = "1" ]; then
    echo "Unable to activate the nanocl group for $E2E_RUNNER_USER" >&2
    exit 1
  fi
  export E2E_NANOCL_PREPARE_SCRIPT="$ROOT_DIR/tests/e2e_prepare_ci.sh"
  if ! command -v sg >/dev/null 2>&1; then
    echo "sg is required to activate the nanocl group" >&2
    exit 1
  fi
  exec sg nanocl -c \
    'E2E_NANOCL_GROUP_REEXEC=1 exec "$E2E_NANOCL_PREPARE_SCRIPT"'
fi

# Set E2E_SKIP_IMAGE_BUILD=1 and/or E2E_SKIP_NANOCL_BUILD=1 for faster local reruns.
if [ "${E2E_SKIP_IMAGE_BUILD:-0}" != "1" ]; then
  NANOCL_CHANNEL="$NANOCL_CHANNEL" sh ./scripts/build_images.sh
fi

if [ "${E2E_SKIP_NANOCL_BUILD:-0}" != "1" ]; then
  NANOCL_CHANNEL="${NANOCL_CHANNEL:-nightly}" cargo build --release --bin nanocl
  sudo cp target/release/nanocl /usr/bin/nanocl
  sudo chmod +x /usr/bin/nanocl
fi

nanocl install -t installer.yml

if [ -d /var/lib/nanocl/store/certs ]; then
  sudo find /var/lib/nanocl/store/certs -type f -name '*.key' -exec chmod 600 {} \; || true
  sudo find /var/lib/nanocl/store/certs -type f -name '*.crt' -exec chmod 644 {} \; || true
fi

if [ -d /var/lib/nanocl/store/ca ]; then
  sudo find /var/lib/nanocl/store/ca -type f -name '*.key' -exec chmod 600 {} \; || true
fi

daemon_ready=0
i=0
while [ "$i" -lt 240 ]; do
  daemon_status="$(unix_health_status \
    /run/nanocl/nanocl.sock http://localhost/v0.0/version)"

  if [ "$daemon_status" = "200" ]; then
    echo "readiness: daemon=200 (ready)"
    daemon_ready=1
    break
  fi

  if [ $((i % 15)) -eq 0 ]; then
    echo "readiness: daemon=${daemon_status}"
  fi

  if ! system_app_id ndaemon >/dev/null; then
    echo "system.ndaemon application container exited unexpectedly" >&2
    log_system_app ndaemon
    exit 1
  fi

  if ! system_app_id ncproxy >/dev/null; then
    echo "system.ncproxy application container exited unexpectedly" >&2
    log_system_app ncproxy
    exit 1
  fi

  i=$((i + 1))
  sleep 1
done

if [ "$daemon_ready" -ne 1 ]; then
  echo "nanocld did not become ready in time" >&2
  docker ps -a
  log_system_app ndaemon
  log_system_app ncproxy
  exit 1
fi

# Wait for ncproxy to bind its socket and accept connections.
proxy_ready=0
p=0
while [ "$p" -lt 240 ]; do
  proxy_status="$(unix_health_status \
    /run/nanocl/proxy.sock http://localhost/health)"
  if [ "$proxy_status" = "200" ]; then
    echo "readiness: proxy=200 (ready)"
    proxy_ready=1
    break
  fi

  if [ $((p % 10)) -eq 0 ]; then
    echo "readiness: proxy=${proxy_status} (${p}s)"
  fi

  if ! system_app_id ncproxy >/dev/null; then
    echo "system.ncproxy application container exited unexpectedly" >&2
    log_system_app ncproxy
    exit 1
  fi

  p=$((p + 1))
  sleep 1
done

if [ "$proxy_ready" -ne 1 ]; then
  echo "ncproxy did not become ready in time" >&2
  diagnose_unix_service ncproxy /run/nanocl/proxy.sock
  docker ps -a
  log_system_app ndaemon
  log_system_app ncproxy
  exit 1
fi

# Wait for ncdns to bind its socket and accept connections.
dns_ready=0
p=0
while [ "$p" -lt 240 ]; do
  dns_status="$(unix_health_status \
    /run/nanocl/dns.sock http://localhost/health)"
  if [ "$dns_status" = "200" ]; then
    echo "readiness: dns=200 (ready)"
    dns_ready=1
    break
  fi

  if [ $((p % 10)) -eq 0 ]; then
    echo "readiness: dns=${dns_status} (${p}s)"
  fi

  if ! system_app_id ncdns >/dev/null; then
    echo "system.ncdns application container exited unexpectedly" >&2
    log_system_app ncdns
    exit 1
  fi

  p=$((p + 1))
  sleep 1
done

if [ "$dns_ready" -ne 1 ]; then
  echo "ncdns did not become ready in time" >&2
  diagnose_unix_service ncdns /run/nanocl/dns.sock
  docker ps -a
  log_system_app ndaemon
  log_system_app ncdns
  exit 1
fi

# GitHub Actions starts each `run` block from its long-lived runner process,
# whose supplementary groups are not refreshed by usermod. Preserve the
# intended nanocl-group check above, then grant only the runner user access for
# the following Bats step instead of making the socket directory world-writable.
if ! command -v setfacl >/dev/null 2>&1; then
  echo "setfacl is required to grant the E2E runner socket access" >&2
  exit 1
fi
sudo setfacl -R -m "u:${E2E_RUNNER_USER}:rwx" /run/nanocl
sudo setfacl -m "d:u:${E2E_RUNNER_USER}:rwx" /run/nanocl

nanocl version
docker ps -a
log_system_app ndaemon
log_system_app ncproxy
log_system_app ncdns

echo "E2E CI prepare complete"
