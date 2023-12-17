#!/bin/sh
## name: build_dev_image.sh

wget https://cloud-images.ubuntu.com/minimal/releases/jammy/release/ubuntu-22.04-minimal-cloudimg-amd64.img
mv ubuntu-22.04-minimal-cloudimg-amd64.img tests/ubuntu-22.04-minimal-cloudimg-amd64.img

# docker pull cockroachdb/cockroach:v23.1.12
# docker pull ghcr.io/nxthat/metrsd:0.3.2
# docker pull ghcr.io/nxthat/nanocl-get-started:latest
# docker pull ghcr.io/nxthat/nanocl-dev:dev
# docker build --network host -t ndns:dev -f ./bin/ndns/Dockerfile .
# docker build --network host -t nproxy:dev -f ./bin/nproxy/Dockerfile .
