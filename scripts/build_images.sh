#!/bin/sh
## name: build_images.sh

# Buildx local command
# BUILDER=buildx-multi-arch
# docker buildx inspect $BUILDER || docker buildx create --name=$BUILDER --driver=docker-container --driver-opt=network=host
# docker buildx build --builder=$(BUILDER) --platform=linux/amd64,linux/arm64 --tag="ghcr.io/next-hat/$name:$version" -f $project/Dockerfile .
# docker buildx build --builder=buildx-multi-arch --platform=linux/amd64,linux/arm/v7,linux/arm64 --tag "ghcr.io/next-hat/nanocl-dev:dev" -f scripts/dev.Dockerfile . --push

REPO=ghcr.io/next-hat

CHANNEL="${NANOCL_CHANNEL:-stable}"

echo "Building images for channel: $CHANNEL"

for project in ./bin/*; do
  ## Extract name from path
  name=$(basename "${project}")
  ## Skip the cli
  if [ "$name" = "nanocl" ]; then
    continue
  fi
  ## Legacy source-only runtime assets are no longer published by default
  if [ "$name" = "nproxy" ] || [ "$name" = "ndns" ]; then
    continue
  fi
  ## Extract version from Cargo.toml
  version=$(grep -m1 version $project/Cargo.toml | sed -e 's/version = //g' | sed -e 's/"//g')
  TAG="$REPO/$name:$version-nightly"
  echo $TAG
  docker buildx build --load --build-arg channel=$CHANNEL -t "ghcr.io/next-hat/$name:$version-nightly" -f $project/Dockerfile .
done
