#!/bin/sh

set -e

## Test is STATE_DIR is empty
if [ -z "$STATE_DIR" ]; then
  echo "STATE_DIR env is not set"
  exit 1
fi

mkdir -p /run/haproxy

## Test if STATE_DIR/haproxy.cfg exists
if [ ! -f "$STATE_DIR/haproxy.cfg" ]; then
  cp /etc/haproxy/haproxy.cfg $STATE_DIR
fi

rm -f /etc/haproxy/haproxy.cfg
ln -s $STATE_DIR/haproxy.cfg /etc/haproxy/haproxy.cfg

echo "Starting haproxy with config: /etc/haproxy/haproxy.cfg"
haproxy -f /etc/haproxy/haproxy.cfg -db -p /run/haproxy.pid
