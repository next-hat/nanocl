# Create Builder image
FROM --platform=$BUILDPLATFORM rust:1.72.1-alpine3.18

# Setup timezone
ARG tz=Europe/Paris

# Install required dependencies
RUN apk add --update alpine-sdk \
  musl-dev \
  g++ \
  make \
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
  cdrkit \
  && rm -rf /var/cache/apk/* \
  && rm -rf /tmp/* \
  && rm -rf /var/log/* \
  && rm -rf /var/tmp/*

RUN cargo install cargo-watch --locked

# Create project directory
RUN mkdir -p /project
WORKDIR /project

ENV TZ=${tz}
ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description The dev image for nanocl services

ENTRYPOINT ["cargo"]
