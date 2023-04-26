#!/bin/sh

sh -c "inotifywait -r /run/nanocl > /dev/null && chmod -R 777 /run/nanocl" &

cargo "$@"
