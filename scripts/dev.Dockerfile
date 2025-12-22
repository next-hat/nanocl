# Create Builder image
FROM --platform=$BUILDPLATFORM rust:1.92.0-alpine3.23

ENV TZ=UTC

RUN apk add --update \
  gcc \
  g++ \
  make

RUN cargo install cargo-watch --locked
RUN cargo install cargo-llvm-cov --locked
RUN rustup component add llvm-tools-preview

# Install required dependencies
RUN apk add --update alpine-sdk \
  musl-dev \
  libpq-dev \
  openssl-dev \
  git \
  perl \
  build-base \
  tzdata \
  util-linux \
  libgcc \
  openssl \
  libpq \
  util-linux \
  bash \
  cloud-utils \
  curl \
  cdrkit

# Create project directory
RUN mkdir -p /project
WORKDIR /project

ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source=https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description="The dev image for nanocl services"

ENTRYPOINT ["cargo"]
