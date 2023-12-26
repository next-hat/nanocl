# Create Builder image
FROM --platform=$BUILDPLATFORM rust:1.74.1-slim-bookworm

# Setup timezone
ARG tz=Europe/Paris

# Install required dependencies
RUN apt-get update && \
  apt-get install -y \
  gcc \
  g++ \
  make

RUN cargo install cargo-watch --locked
RUN cargo install cargo-llvm-cov --locked
RUN rustup component add llvm-tools-preview

RUN apt-get install -y \
  pkg-config \
  musl-dev \
  libpq-dev \
  libssl-dev \
  musl-tools \
  libc-dev \
  qemu-utils \
  nginx \
  nginx-extras \
  dnsmasq \
  cron

# Create project directory
RUN mkdir -p /project
WORKDIR /project

ENV TZ=${tz}
ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description The dev image for nanocl services

ENTRYPOINT ["cargo"]
