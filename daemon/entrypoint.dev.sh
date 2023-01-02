#!/bin/sh

chmod -R 777 /run/nanocl
exec runuser -u nanocl -- $@
