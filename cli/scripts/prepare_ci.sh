#!/bin/sh
## name: ci_install.sh

useradd -U nanocl;
usermod -aG nanocl $USER;
mkdir -p /var/lib/nanocl/nginx/sites-enabled
