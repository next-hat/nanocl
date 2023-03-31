#!/bin/sh
## name: build_dev_image.sh

docker build -t nanocl-dev:dev -f ./dev.Dockerfile .
docker build -t nanocl-dns:dev -f ./bin/ctrl-dns/dnsmasq/Dockerfile ./bin/ctrl-dns/dnsmasq
docker build -t nanocl-proxy:dev -f ./bin/ctrl-proxy/nginx/Dockerfile ./bin/ctrl-proxy/nginx
