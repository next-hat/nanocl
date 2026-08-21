#!/usr/bin/env bash
# Prepare a local/CI environment for E2E tests.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

NANOCL_CHANNEL="${NANOCL_CHANNEL:-nightly}"

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

# Set E2E_SKIP_IMAGE_BUILD=1 and/or E2E_SKIP_NANOCL_BUILD=1 for faster local reruns.
if [ "${E2E_SKIP_IMAGE_BUILD:-0}" != "1" ]; then
  NANOCL_CHANNEL="$NANOCL_CHANNEL" sh ./scripts/build_images.sh
fi

if [ "${E2E_SKIP_NANOCL_BUILD:-0}" != "1" ]; then
  NANOCL_CHANNEL="${NANOCL_CHANNEL:-nightly}" cargo build --release --bin nanocl
  sudo cp target/release/nanocl /usr/bin/nanocl
  sudo chmod +x /usr/bin/nanocl
fi

if ! getent group nanocl >/dev/null 2>&1; then
  sudo groupadd nanocl
fi
sudo usermod -aG nanocl "$USER" || true
sudo gpasswd -r nanocl || true

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
  daemon_status=$(sudo curl --silent --output /dev/null --write-out "%{http_code}" --unix-socket /run/nanocl/nanocl.sock http://localhost/v0.0/version || true)

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
  if [ -S /run/nanocl/proxy.sock ]; then
    proxy_status=$(sudo curl --silent --output /dev/null --write-out "%{http_code}" --unix-socket /run/nanocl/proxy.sock 'http://localhost/health' 2>/dev/null || true)
    if [ "$proxy_status" != "000" ]; then
      echo "readiness: proxy=${proxy_status} (ready)"
      proxy_ready=1
      break
    fi
  fi

  if [ $((p % 10)) -eq 0 ]; then
    echo "readiness: proxy=waiting (${p}s)"
  fi

  if ! system_app_id ncproxy >/dev/null; then
    echo "system.ncproxy application container exited unexpectedly" >&2
    log_system_app ncproxy
    exit 1
  fi

  p=$((p + 1))
  sleep 1
done

proxy_id="$(system_app_id ncproxy)"

sudo ls -la /run/nanocl || true
sudo stat /run/nanocl/proxy.sock || true

docker exec "$proxy_id" sh -c \
  'id; ls -la /run/nanocl; grep proxy.sock /proc/net/unix || true'

docker inspect "$proxy_id" \
  --format '{{range .Mounts}}{{println .Source "->" .Destination}}{{end}}'

sudo curl --show-error --verbose \
  --unix-socket /run/nanocl/proxy.sock \
  http://localhost/health || true

if [ "$proxy_ready" -ne 1 ]; then
  echo "ncproxy did not become ready in time" >&2
  docker ps -a
  log_system_app ndaemon
  log_system_app ncproxy
  exit 1
fi

# Wait for ncdns to bind its socket and accept connections.
dns_ready=0
p=0
while [ "$p" -lt 240 ]; do
  if [ -S /run/nanocl/dns.sock ]; then
    dns_status=$(sudo curl --silent --output /dev/null --write-out "%{http_code}" --unix-socket /run/nanocl/dns.sock 'http://localhost/health' 2>/dev/null || true)
    if [ "$dns_status" != "000" ]; then
      echo "readiness: dns=${dns_status} (ready)"
      dns_ready=1
      break
    fi
  fi

  if [ $((p % 10)) -eq 0 ]; then
    echo "readiness: dns=waiting (${p}s)"
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
  docker ps -a
  log_system_app ndaemon
  log_system_app ncdns
  exit 1
fi

sudo chmod 777 -R /run/nanocl

nanocl version
docker ps -a
log_system_app ndaemon
log_system_app ncproxy
log_system_app ncdns

echo "E2E CI prepare complete"
