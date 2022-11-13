#!/bin/sh
## name: ci_install.sh

useradd -U nanocl;
mkdir /etc/nanocl;
chmod 777 /etc/nanocl;
mkdir /run/nanocl;
chmod 777 /run/nanocl;
echo "docker_host: /run/docker.sock" > /etc/nanocl/nanocl.conf;
cat /etc/nanocl/nanocl.conf;
