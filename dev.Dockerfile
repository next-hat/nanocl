# Create Builder image
FROM rust:1.69.0-alpine3.17

# Setup timezone
ARG tz=Europe/Paris

# Install required dependencies
RUN apk add --update alpine-sdk musl-dev g++ make libpq-dev openssl-dev git perl build-base tzdata util-linux libgcc openssl libpq util-linux bash cloud-utils cdrkit

# Install cargo-watch
RUN cargo install cargo-watch --locked

# Create project directory
RUN mkdir -p /project
WORKDIR /project

COPY ./dev.entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV TZ=${tz}
ENV RUSTFLAGS="-C target-feature=-crt-static"

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description The dev image for nanocl services

ENTRYPOINT ["/entrypoint.sh"]
