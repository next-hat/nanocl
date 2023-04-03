#!/bin/sh
## name: build_dev_image.sh

docker pull cockroachdb/cockroach:v22.2.6
docker pull nexthat/metrsd:v0.1.0
docker pull nexthat/nanocl-get-started:latest
docker build -t nanocl-dev:dev -f ./dev.Dockerfile .
docker build -t nanocl-dns:dev -f ./bin/ctrl-dns/dnsmasq/Dockerfile ./bin/ctrl-dns/dnsmasq
docker build -t nanocl-proxy:dev -f ./bin/ctrl-proxy/nginx/Dockerfile ./bin/ctrl-proxy/nginx
