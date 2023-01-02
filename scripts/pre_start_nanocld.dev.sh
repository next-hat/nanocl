#!/bin/sh -i
## name: pre_start_nanocl.dev.sh
set -e -x

: ${bridge=nanocl0}

# Set up bridge network:
if ! ip link show $bridge > /dev/null 2>&1
then
   ip link add name $bridge type bridge
   ip addr add ${net:-"142.0.0.1/24"} dev $bridge
   ip link set dev $bridge up
fi

mkdir -p /run/nanocl
mkdir -p /var/lib/nanocl

containerd --config ./fake_path/etc/nanocl/containerd.conf &> /dev/null 2>&1
dockerd --config-file ./fake_path/etc/nanocl/dockerd.json &> /dev/null 2>&1

chmod 777 -R /run/nanocl
chmod 777 -R /var/lib/nanocl
