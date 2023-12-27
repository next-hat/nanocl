#!/bin/sh

if [ -z "$STATE_DIR" ]; then
  echo "STATE_DIR env is not set"
  exit 1
fi

# Test if a config file exists at $STATE_DIR/dnsmasq.conf
if [ ! -f $STATE_DIR/dnsmasq.conf ]; then
  echo "No config file found at $STATE_DIR/dnsmasq.conf loading default config"
  cp /dnsmasq.conf $STATE_DIR/dnsmasq.conf
fi

# Test if directory $STATE_DIR/dnsmasq.d exists
if [ ! -d $STATE_DIR/dnsmasq.d ]; then
  echo "No directory found at $STATE_DIR/dnsmasq.d creating directory"
  mkdir $STATE_DIR/dnsmasq.d
fi

# Start dnsmasq
dnsmasq -k -C $STATE_DIR/dnsmasq.conf --log-facility=-
