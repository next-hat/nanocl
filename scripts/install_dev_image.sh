#!/bin/sh
## name: build_dev_image.sh

docker pull cockroachdb/cockroach:v22.2.6
docker pull nexthat/metrsd:v0.1.0
docker pull nexthat/nanocl-get-started:latest
docker pull ghcr.io/nxthat/nanocl-dev:dev
docker build -t nanocl-dns:dev -f ./bin/ncddns/dnsmasq/Dockerfile ./bin/ncddns/dnsmasq
docker build -t nanocl-proxy:dev -f ./bin/ncdproxy/nginx/Dockerfile ./bin/ncdproxy/nginx
