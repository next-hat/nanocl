#!/bin/sh
## name: build_images.sh

BUILDER=buildx-multi-arch

docker buildx inspect $BUILDER || docker buildx create --name=$BUILDER --driver=docker-container --driver-opt=network=host

for project in ./bin/*; do
  name=$(basename "${project}")
  version=$(cat $project/version)
  # if [ "$name" != "nanocld" ]; then
  #   continue
  # fi
  echo "Building ${name}:${version}"
  docker build -t "ghcr.io/nxthat/$name:$version-nightly" -f $project/Dockerfile .
  # docker buildx build --builder=$(BUILDER) --platform=linux/amd64,linux/arm64 --tag="ghcr.io/nxthat/$name:$version" -f $project/Dockerfile .
done

echo "Done!"
