# stage 1 - Install cargo chef
from rust:1.65.0-alpine3.16 as cargo-chef

WORKDIR /app
RUN apk add gcc g++ make
RUN cargo install cargo-chef --locked

# stage 2 - Prepare recipe
from rust:1.65.0-alpine3.16 as planner

WORKDIR /app
COPY --from=cargo-chef /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY ./Cargo.lock ./Cargo.toml ./
RUN cargo chef prepare --recipe-path recipe.json

# stage 3 - build our dependencies
from rust:1.65.0-alpine3.16 as cacher

WORKDIR /app
COPY --from=planner /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
COPY --from=planner /app .
RUN apk add openssl gcc g++
RUN cargo chef cook --release --recipe-path recipe.json

# stage 4 - build our project
from rust:1.65.0-alpine3.16 as builder

WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app .
COPY ./src ./src
COPY ./build.rs .
RUN cargo build --release

# stage 4 - create runtime image
from alpine:3.16.2

COPY --from=builder /app/target/release/nanocl /usr/local/bin/nanocl
COPY ./entrypoint.sh /entrypoint.sh
RUN apk add util-linux

ENTRYPOINT ["/entrypoint.sh"]
