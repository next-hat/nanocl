#!/bin/sh
## name: ci_install.sh

sudo useradd -U nanocl;
sudo usermod -aG nanocl $USER;
newgrp nanocl;
sudo mkdir /etc/nanocl;
sudo chmod 770 /etc/nanocl;
sudo chown :nanocl /etc/nanocl;
echo "docker_host: /run/docker.sock\n" > /etc/nanocl/nanocl.conf;
sudo chown :nanocl /etc/nanocl/nanocl.conf;
sudo chmod 770 /etc/nanocl/nanocl.conf;
sudo chown :nanocl /run/docker.sock;
