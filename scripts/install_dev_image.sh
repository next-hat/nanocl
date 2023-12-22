#!/bin/sh
## name: build_dev_image.sh

echo "Downloading vm images.."
wget https://cloud-images.ubuntu.com/minimal/releases/jammy/release/ubuntu-22.04-minimal-cloudimg-amd64.img
mv ubuntu-22.04-minimal-cloudimg-amd64.img tests/ubuntu-22.04-minimal-cloudimg-amd64.img

echo "Downloading container images.."
docker pull cockroachdb/cockroach:v23.1.12
docker pull ghcr.io/next-hat/metrsd:0.3.2
docker pull ghcr.io/next-hat/nanocl-get-started:latest
docker pull ghcr.io/next-hat/nanocl-dev:dev
docker build --network host -t ndns:dev -f ./bin/ndns/Dockerfile .
docker build --network host -t nproxy:dev -f ./bin/nproxy/Dockerfile .
