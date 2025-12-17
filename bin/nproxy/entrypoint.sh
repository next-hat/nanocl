#!/bin/sh

set -e

## Test is STATE_DIR is empty
if [ -z "$STATE_DIR" ]; then
  echo "STATE_DIR env is not set"
  exit 1
fi

mkdir -p /run/haproxy

## Create default haproxy.cfg if it doesn't exist in STATE_DIR
if [ ! -f "$STATE_DIR/haproxy.cfg" ]; then
  echo "Creating default haproxy.cfg"
  cat > "$STATE_DIR/haproxy.cfg" <<'EOF'
global
  log stdout format raw local0 info
  maxconn 2048

defaults
  log global
  mode http
  option dontlognull
  option forwardfor
  timeout connect 5s
  timeout client  50s
  timeout server  50s
EOF
fi

## Create empty frontends.cfg and streams.cfg if they don't exist
if [ ! -f "$STATE_DIR/frontends.cfg" ]; then
  echo "Creating default frontends.cfg with minimal HTTP frontend"
  cat > "$STATE_DIR/frontends.cfg" <<'EOF'

frontend http_80
  bind *:80
  mode http
  option forwardfor
  http-request return status 503 content-type text/html lf-string "<html><body><h1>503 Service Unavailable</h1><p>Proxy is starting...</p></body></html>"

EOF
fi

if [ ! -f "$STATE_DIR/streams.cfg" ]; then
  touch "$STATE_DIR/streams.cfg"
fi

echo "Starting haproxy with config: $STATE_DIR/haproxy.cfg, $STATE_DIR/frontends.cfg, $STATE_DIR/streams.cfg"
haproxy -W -f $STATE_DIR/haproxy.cfg -f $STATE_DIR/frontends.cfg -f $STATE_DIR/streams.cfg -p /run/haproxy.pid
