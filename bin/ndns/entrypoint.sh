#!/bin/sh

# Test if a config file exists at /opt/dns/dnsmasq.conf
if [ ! -f /opt/dns/dnsmasq.conf ]; then
  echo "No config file found at /opt/dns/dnsmasq.conf loading default config"
  mv /dnsmasq.conf /opt/dns/dnsmasq.conf
fi

# Test if directory /opt/dns/dnsmasq.d exists
if [ ! -d /opt/dns/dnsmasq.d ]; then
  echo "No directory found at /opt/dns/dnsmasq.d creating directory"
  mkdir /opt/dns/dnsmasq.d
fi

# Start dnsmasq
dnsmasq -k -C /opt/dns/dnsmasq.conf --log-facility=-
