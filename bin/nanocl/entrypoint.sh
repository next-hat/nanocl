#!/bin/sh

getent group nanocl > /dev/null 2&>1

if [ $? -ne 0 ]; then
  addgroup -S nanocl -g $NANOCL_GID
fi

exec runuser -u root -g nanocl -- /usr/local/bin/nanocl $@
