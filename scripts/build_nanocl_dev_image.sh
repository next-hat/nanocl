# Buildx local command
BUILDER=buildx-multi-arch
docker buildx inspect $BUILDER || docker buildx create --name=$BUILDER --driver=docker-container --driver-opt=network=host
# docker buildx build --builder=$(BUILDER) --platform=linux/amd64,linux/arm64 --tag="ghcr.io/next-hat/$name:$version" -f $project/Dockerfile .
docker buildx build --builder=buildx-multi-arch --platform=linux/amd64,linux/arm/v7,linux/arm64 --tag "ghcr.io/next-hat/nanocl-dev:dev" -f scripts/dev.Dockerfile . --load --push
