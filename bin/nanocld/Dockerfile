# stage 1 - Setup cargo-chef
FROM rust:1.67.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json --bin ./bin/nanocld

# state 2 - Cook our dependencies
FROM rust:1.67.0-alpine3.17 as cacher

WORKDIR /app
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY --from=planner /app .
RUN apk add musl-dev libpq-dev openssl-dev
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo chef cook --release --recipe-path recipe.json --bin nanocld

# stage 3 - Build our project
FROM rust:1.67.0-alpine3.17 as builder

## Build our nanocl daemon binary
WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app .
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY .git ./.git
RUN apk add musl-dev libpq-dev openssl-dev git upx
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release --bin nanocld

## Strip and compress the binary
RUN strip target/release/nanocld
RUN upx /app/target/release/nanocld

# stage 4 - Create runtime image
FROM alpine:3.17

## Install dependencies
RUN apk add libgcc libpq util-linux

## Copy the binary
COPY --from=builder /app/target/release/nanocld /usr/local/bin/nanocld
COPY ./bin/nanocld/entrypoint.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh

## Set entrypoint
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
