#!/bin/sh

getent group nanocl > /dev/null 2&>1

if [ $? -ne 0 ]; then
  addgroup -S nanocl -g $NANOCL_GID
  chown root:nanocl -R /run/nanocl
  chmod -R 770 /run/nanocl
fi

sh -c "sleep 4 && chmod -R 770 /run/nanocl" &

exec runuser -u root -g nanocl -- /usr/local/bin/nanocld $@
