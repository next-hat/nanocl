#!/bin/bash

getent group nanocl > /dev/null 2&>1

if [ $? -ne 0 ]; then
  addgroup -S nanocl -g $NANOCL_GID
  chown root:nanocl -R /run/nanocl
  chmod -R 770 /run/nanocl
fi

exec runuser -u root -g nanocl -- /usr/local/bin/nanocld $@
