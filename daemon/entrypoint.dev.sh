#!/bin/sh

chown -R :ping /run/nanocl
chmod -R 770 /run/nanocl
exec runuser -u nanocl -- $@
