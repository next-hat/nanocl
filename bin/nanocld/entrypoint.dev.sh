#!/bin/bash

getent group nanocl > /dev/null 2&>1

if [ $? -ne 0 ]; then
  addgroup -S nanocl -g $NANOCL_GID
  chown root:nanocl -R /run/nanocl
  chmod -R 770 /run/nanocl
fi


INIT=false

for ARG in "$@"
do
  if [[ $ARG == "--init" ]]; then
    INIT=true
    break
  fi
done

if [[ $INIT == false ]]; then
  sh -c "inotifywait -e create /run/nanocl > /dev/null &&
    chown root:nanocl -R /run/nanocl && chmod -R 770 /run/nanocl" &
fi

exec runuser -u root -g nanocl -- cargo watch "$@"
