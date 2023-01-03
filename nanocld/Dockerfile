# stage 1 - generate recipe file for dependencies
from rust:1.64.0-alpine3.16 as planner

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked
COPY ./Cargo.lock ./Cargo.toml ./
RUN cargo chef prepare --recipe-path recipe.json

# state 2 - build our dependencies
from rust:1.64.0-alpine3.16 as cacher
WORKDIR /app
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY --from=planner /app .
RUN apk add musl-dev libpq-dev
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo chef cook --release --recipe-path recipe.json

# stage 3 - build our project
from rust:1.64.0-alpine3.16 as builder
WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app .
COPY ./migrations ./migrations
COPY ./src ./src
COPY ./build.rs .
RUN apk add musl-dev libpq-dev
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release

# stage 4 - create runtime image
from alpine:3.16.2

RUN apk add libgcc libpq util-linux

COPY --from=builder /app/target/release/nanocld /usr/local/bin/nanocld

COPY entrypoint.sh /entrypoint.sh

ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
