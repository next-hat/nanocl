# Create Builder image
FROM --platform=$BUILDPLATFORM rust:1.96.0-alpine3.23

ENV TZ=UTC

RUN apk add --update \
  alpine-sdk \
  bash \
  build-base \
  ca-certificates \
  cdrkit \
  cloud-utils \
  curl \
  dnsmasq \
  git \
  libgcc \
  libpq \
  libpq-dev \
  musl-dev \
  nginx \
  nginx-mod-http-headers-more \
  nginx-mod-http-lua \
  nginx-mod-stream \
  openssl \
  openssl-dev \
  perl \
  procps-ng \
  tzdata \
  util-linux

RUN cargo install cargo-watch --locked
RUN cargo install cargo-llvm-cov --locked
RUN rustup component add llvm-tools-preview

# Create project directory
RUN mkdir -p /project
WORKDIR /project

ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source=https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description="The dev image for nanocl services"

ENTRYPOINT ["cargo"]
