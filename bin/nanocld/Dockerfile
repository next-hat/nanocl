# stage 1 - generate recipe file for dependencies
FROM rust:1.67.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked
COPY ./bin/nanocld/Cargo.toml ./
RUN cargo chef prepare --recipe-path recipe.json

# state 2 - build our dependencies
FROM rust:1.67.0-alpine3.17 as cacher
WORKDIR /app
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY --from=planner /app .
RUN apk add musl-dev libpq-dev
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo chef cook --release --recipe-path recipe.json

# stage 3 - build our project
FROM rust:1.67.0-alpine3.17 as builder
WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app .
COPY ./bin/nanocld/migrations ./migrations
COPY ./bin/nanocld/src ./src
COPY ./bin/nanocld/build.rs .
COPY .git ./.git
RUN apk add musl-dev libpq-dev git
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release

# stage 4 - create runtime image
FROM alpine:3.17

RUN apk add libgcc libpq util-linux

COPY --from=builder /app/target/release/nanocld /usr/local/bin/nanocld

COPY ./bin/nanocld/entrypoint.sh /entrypoint.sh

ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
