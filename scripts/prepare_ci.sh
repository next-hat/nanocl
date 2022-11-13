#!/bin/sh
## name: ci_install.sh

useradd -U nanocl;
usermod -aG nanocl $USER;
mkdir /etc/nanocl;
chmod 777 /etc/nanocl;
mkdir /run/nanocl;
chmod 777 /run/nanocl;
echo "docker_host: /run/docker.sock\n" > /etc/nanocl/nanocl.conf;
cat /etc/nanocl/nanocl.conf;

