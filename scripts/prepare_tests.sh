#!/bin/sh
## name: ci_install.sh

groupadd nanocl;
usermod -aG nanocl $USER;
mkdir -p /run/nanocl
chmod 777 -R /run/nanocl
mkdir -p /var/lib/nanocl/vms/images
mkdir -p /var/lib/nanocl/nginx/sites-enabled
chmod 777 -R /var/lib/nanocl
