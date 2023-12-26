#!/bin/sh

set -e

## Test is STATE_DIR is empty
if [ -z "$STATE_DIR" ]; then
  echo "STATE_DIR env is not set"
  exit 1
fi

mkdir -p /var/log/nginx

## Test if STATE_DIR/nginx.conf exists
if [ ! -f "$STATE_DIR/nginx.conf" ]; then
  cp /etc/nginx/nginx.conf $STATE_DIR
fi

rm -f /etc/nginx/nginx.conf
ln -s $STATE_DIR/nginx.conf /etc/nginx/nginx.conf

nginx -g "daemon off;"
