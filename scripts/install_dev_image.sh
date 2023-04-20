#!/bin/sh
## name: build_dev_image.sh

docker pull cockroachdb/cockroach:v22.2.7
docker pull ghcr.io/nxthat/metrsd:0.2.0
docker pull nexthat/nanocl-get-started:latest
docker pull ghcr.io/nxthat/nanocl-dev:dev
docker build -t ndns:dev -f ./bin/ncddns/dnsmasq/Dockerfile ./bin/ncddns/dnsmasq
docker build -t nproxy:dev -f ./bin/nproxy/Dockerfile .
