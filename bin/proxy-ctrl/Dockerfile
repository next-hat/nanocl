# stage 1 - generate recipe file for dependencies
FROM rust:1.68.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml
COPY ./bin/proxy-ctrl/Cargo.toml ./bin/proxy-ctrl/Cargo.toml

# stage 2 - Build our project
FROM rust:1.68.0-alpine3.17 as builder

ARG channel=stable

## Build our nanocl daemon binary
COPY --from=planner /app /app
WORKDIR /app
RUN apk add musl-dev libpq-dev openssl git upx
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY ./crates/nanocld_client/src ./crates/nanocld_client/src
COPY ./bin/nanocl/src ./bin/nanocl/src
COPY ./bin/nanocl/build.rs ./bin/nanocl/build.rs
COPY .git ./.git
COPY ./bin/proxy-ctrl/src ./bin/proxy-ctrl/src
COPY ./bin/proxy-ctrl/build.rs ./bin/proxy-ctrl/build.rs
COPY ./bin/proxy-ctrl/specs ./bin/proxy-ctrl/specs
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target=x86_64-unknown-linux-musl --bin proxy-ctrl
RUN upx --lzma --best /app/target/x86_64-unknown-linux-musl/release/proxy-ctrl

# stage 4 - create runtime image
FROM scratch

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/proxy-ctrl /bin/proxy-ctrl

ENTRYPOINT ["proxy-ctrl"]
