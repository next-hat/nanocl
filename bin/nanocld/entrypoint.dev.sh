#!/bin/bash

# If dockerd exists we start it
if [ -x "$(command -v dockerd)" ]; then
    dockerd 2> /dev/null &
    sleep 5
fi

sh -c "inotifywait -e create /run/nanocl > /dev/null && chmod -R 777 /run/nanocl" &

exec "$@"
