# stage 1 - Setup dependencies
FROM rust:1.68.2-alpine3.17 as planner

WORKDIR /app
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/ctrl-proxy/Cargo.toml ./bin/ctrl-proxy/Cargo.toml
COPY ./bin/ctrl-dns/Cargo.toml ./bin/ctrl-dns/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml

# stage 2 - Build binary
FROM rust:1.68.2-alpine3.17 as builder

ARG channel=stable

## Build our nanocl daemon binary
COPY --from=planner /app /app
WORKDIR /app
RUN apk add --update alpine-sdk musl-dev g++ make libpq-dev openssl-dev git upx perl build-base
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./bin/nanocld/specs ./bin/nanocld/specs
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY ./crates/nanocld_client/src ./crates/nanocld_client/src
COPY ./bin/nanocl/src ./bin/nanocl/src
COPY ./bin/nanocl/build.rs ./bin/nanocl/build.rs
COPY .git ./.git
ENV NANOCL_CHANNEL=$channel
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target=x86_64-unknown-linux-musl --bin nanocld
RUN upx --lzma --best /app/target/x86_64-unknown-linux-musl/release/nanocld

# stage 3 - Create runtime image
FROM alpine:3.17.3

## Install dependencies
RUN apk update && apk upgrade
RUN apk add util-linux inotify-tools bash cloud-utils cdrkit

## Copy the binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/nanocld /usr/local/bin/nanocld
COPY ./bin/nanocld/entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh


LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description The nanocl daemon

## Set entrypoint
ENTRYPOINT ["/bin/bash", "/entrypoint.sh"]
