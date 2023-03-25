# stage 1 - generate recipe file for dependencies
FROM rust:1.67.0-alpine3.17 as planner

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked
COPY ./Cargo.toml ./Cargo.lock ./
RUN cargo chef prepare --recipe-path recipe.json

# state 2 - build our dependencies
FROM rust:1.67.0-alpine3.17 as cacher

WORKDIR /app
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY --from=planner /app .
RUN apk update && apk upgrade
RUN apk add gcc g++ make
RUN cargo chef cook --release --target=x86_64-unknown-linux-musl --recipe-path recipe.json

# stage 3 - build our project
FROM rust:1.67.0-alpine3.17 as builder

WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app .
COPY ./src ./src
RUN apk update && apk upgrade
RUN apk add gcc g++ make upx
RUN cargo build --release --target=x86_64-unknown-linux-musl
RUN strip /app/target/x86_64-unknown-linux-musl/release/nanocl-ctrl-dns
RUN upx /app/target/x86_64-unknown-linux-musl/release/nanocl-ctrl-dns

# stage 4 - create runtime image
from scratch

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/nanocl-ctrl-dns /bin/nanocl-ctrl-dns

ENTRYPOINT ["nanocl-ctrl-dns"]
