# stage 1 - generate recipe file for dependencies
FROM rust:1.69.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc make
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocl_utils/Cargo.toml ./crates/nanocl_utils/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml
COPY ./bin/ncdproxy/Cargo.toml ./bin/ncdproxy/Cargo.toml
COPY ./bin/ncddns/Cargo.toml ./bin/ncddns/Cargo.toml

# stage 2 - Build our project
FROM rust:1.69.0-alpine3.17 as builder

ARG channel=stable

## Build our nanocl daemon binary
COPY --from=planner /app /app
WORKDIR /app
RUN rustup target add x86_64-unknown-linux-musl
RUN apk add --update alpine-sdk musl-dev g++ make libpq-dev openssl-dev git upx perl build-base
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY ./crates/nanocl_utils/src ./crates/nanocl_utils/src
COPY ./crates/nanocld_client/src ./crates/nanocld_client/src
COPY ./bin/nanocl/src ./bin/nanocl/src
COPY ./bin/nanocl/build.rs ./bin/nanocl/build.rs
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/nanocld/specs ./bin/nanocld/specs
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./bin/ncdproxy/src ./bin/ncdproxy/src
COPY ./bin/ncdproxy/build.rs ./bin/ncdproxy/build.rs
COPY ./bin/ncdproxy/specs ./bin/ncdproxy/specs
COPY ./bin/ncddns/build.rs ./bin/ncddns/build.rs
COPY ./bin/ncddns/src ./bin/ncddns/src
COPY .git ./.git
ENV NANOCL_CHANNEL=$channel
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target=x86_64-unknown-linux-musl --features release --bin ncddns
RUN upx --lzma --best /app/target/x86_64-unknown-linux-musl/release/ncddns

# stage 4 - create runtime image
FROM scratch

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description Nanocl Controller DNS

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/ncddns /bin/ncddns

ENTRYPOINT ["ncddns"]
