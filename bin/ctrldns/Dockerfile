# stage 1 - generate recipe file for dependencies
FROM rust:1.68.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc make
RUN cargo install cargo-chef --locked
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml
COPY ./bin/ctrlproxy/Cargo.toml ./bin/ctrlproxy/Cargo.toml
COPY ./bin/ctrldns/Cargo.toml ./bin/ctrldns/Cargo.toml

# stage 2 - Build our project
FROM rust:1.68.0-alpine3.17 as builder

ARG channel=stable

## Build our nanocl daemon binary
COPY --from=planner /app /app
WORKDIR /app
RUN rustup target add x86_64-unknown-linux-musl
RUN apk update && apk upgrade
RUN apk add clang-dev gcc gcompat make musl-dev libpq-dev openssl openssl-dev git upx
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/nanocld/specs ./bin/nanocld/specs
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY ./crates/nanocld_client/src ./crates/nanocld_client/src
COPY ./bin/nanocl/src ./bin/nanocl/src
COPY ./bin/nanocl/build.rs ./bin/nanocl/build.rs
COPY .git ./.git
COPY ./bin/ctrlproxy/src ./bin/ctrlproxy/src
COPY ./bin/ctrlproxy/build.rs ./bin/ctrlproxy/build.rs
COPY ./bin/ctrlproxy/specs ./bin/ctrlproxy/specs
COPY ./bin/ctrldns/src ./bin/ctrldns/src
RUN cargo build --release --target=x86_64-unknown-linux-musl --features release --bin ctrl-dns
RUN upx --lzma --best /app/target/x86_64-unknown-linux-musl/release/ctrl-dns

# stage 4 - create runtime image
FROM scratch

LABEL org.opencontainers.image.source https://github.com/nxthat/nanocl
LABEL org.opencontainers.image.description The nanocl DNS controller

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/ctrl-dns /bin/ctrldns

ENTRYPOINT ["ctrl-dns"]
