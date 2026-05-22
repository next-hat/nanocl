# Create Builder image
FROM --platform=$BUILDPLATFORM rust:1.95.0-bookworm

ENV TZ=UTC

RUN apt-get update && \
  apt-get install -y \
  bash \
  build-essential \
  ca-certificates \
  cloud-image-utils \
  curl \
  dnsmasq \
  genisoimage \
  git \
  libpq-dev \
  libssl-dev \
  make \
  nginx \
  nginx-common \
  nginx-extras \
  perl \
  procps \
  tzdata \
  util-linux && \
  rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-watch --locked
RUN cargo install cargo-llvm-cov --locked
RUN rustup component add llvm-tools-preview

# Create project directory
RUN mkdir -p /project
WORKDIR /project

LABEL org.opencontainers.image.source=https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description="The dev image for nanocl services"

ENTRYPOINT ["cargo"]
