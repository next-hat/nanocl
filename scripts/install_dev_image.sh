#!/bin/sh
## name: build_dev_image.sh

if [ ! -f "tests/ubuntu-24.04-minimal-cloudimg-amd64.img" ]; then
  echo "Downloading vm images.."
  wget https://cloud-images.ubuntu.com/minimal/releases/noble/release/ubuntu-24.04-minimal-cloudimg-amd64.img
  mv ubuntu-24.04-minimal-cloudimg-amd64.img tests/ubuntu-24.04-minimal-cloudimg-amd64.img
fi

echo "Downloading container images.."
docker pull docker.io/cockroachdb/cockroach:v24.1.0
docker pull ghcr.io/next-hat/metrsd:0.5.4
docker pull ghcr.io/next-hat/nanocl-get-started:latest
docker pull ghcr.io/next-hat/nanocl-dev:dev
docker buildx build --load -t ndns:dev -f ./bin/ndns/Dockerfile .
docker buildx build --load -t nproxy:dev -f ./bin/nproxy/Dockerfile .
