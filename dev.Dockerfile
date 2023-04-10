# Create Builder image
FROM rust:1.68.0-alpine3.17

# Setup timezone
ARG tz=Europe/Paris


# Install required dependencies
RUN apk add --update alpine-sdk musl-dev g++ make libpq-dev openssl-dev git upx perl build-base tzdata bash util-linux libgcc openssl libpq util-linux inotify-tools bash cloud-utils cdrkit

# Install cargo-watch
RUN cargo install cargo-watch --locked

# Create project directory
RUN mkdir -p /project
WORKDIR /project

ENV TZ=${tz}
ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description The dev image for nanocl services

ENTRYPOINT ["cargo"]
