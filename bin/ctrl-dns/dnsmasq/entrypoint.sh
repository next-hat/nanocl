#!/bin/sh

# Test if a config file exists at /etc/dnsmasq.conf
if [ ! -f /etc/dnsmasq.conf ]; then
  echo "No config file found at /etc/dnsmasq.conf loading default config"
  mv /dnsmasq.conf /etc/dnsmasq.conf
fi

# Start dnsmasq
dnsmasq -k -C /etc/dnsmasq.conf --log-facility=-
