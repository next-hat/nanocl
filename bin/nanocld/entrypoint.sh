#!/bin/sh

getent group nanocl > /dev/null 2&>1

if [ $? -ne 0 ]; then
  addgroup -S nanocl -g $NANOCL_GID
  chown root:nanocl -R /run/nanocl
  chmod -R 770 /run/nanocl
fi

init=false

for arg in "$@"; do
  if ["$arg" = "--init"]; then
    init=true
    break
  fi
done

exec runuser -u root -g nanocl -- /usr/local/bin/nanocld $@ &

if [ "$init" = true ]; then
  exit 0
fi

inotifywait -e create /run/nanocl > /dev/null
chown root:nanocl -R /run/nanocl
chmod -R 770 /run/nanocl

wait
