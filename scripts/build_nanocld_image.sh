#!/bin/sh
## name: build_nanocld_image.sh

docker build -t nanocld:nightly -f ./bin/nanocld/Dockerfile .
