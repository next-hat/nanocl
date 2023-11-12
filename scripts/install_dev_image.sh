#!/bin/sh
## name: build_dev_image.sh

docker pull cockroachdb/cockroach:v23.1.11
docker pull ghcr.io/nxthat/metrsd:0.3.1
docker pull ghcr.io/nxthat/nanocl-get-started:latest:latest
docker pull ghcr.io/nxthat/nanocl-dev:dev
docker build --network host -t ndns:dev -f ./bin/ndns/Dockerfile .
docker build --network host -t nproxy:dev -f ./bin/nproxy/Dockerfile .
