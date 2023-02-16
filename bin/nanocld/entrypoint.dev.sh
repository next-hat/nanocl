#!/bin/sh

exec cargo watch $@

sh -c "sleep 4 && chmod -R 777 /run/nanocl"

wait
