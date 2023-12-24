# stage 1 - generate recipe file for dependencies
FROM --platform=$BUILDPLATFORM rust:1.74.1-alpine3.18 as planner

WORKDIR /app
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./crates/nanocl_error/Cargo.toml ./crates/nanocl_error/Cargo.toml
COPY ./crates/nanocl_stubs/Cargo.toml ./crates/nanocl_stubs/Cargo.toml
COPY ./crates/nanocl_utils/Cargo.toml ./crates/nanocl_utils/Cargo.toml
COPY ./crates/nanocld_client/Cargo.toml ./crates/nanocld_client/Cargo.toml
COPY ./bin/nanocl/Cargo.toml ./bin/nanocl/Cargo.toml
COPY ./bin/nanocld/Cargo.toml ./bin/nanocld/Cargo.toml
COPY ./bin/ncproxy/Cargo.toml ./bin/ncproxy/Cargo.toml
COPY ./bin/ncvpnkit/Cargo.toml ./bin/ncvpnkit/Cargo.toml
COPY ./bin/ncdns/Cargo.toml ./bin/ncdns/Cargo.toml

# stage 2 - Build our project
FROM --platform=$BUILDPLATFORM rust:1.74.1-alpine3.18 as builder

ARG channel=stable
## Build our nanocl daemon binary
COPY --from=planner /app /app
WORKDIR /app
RUN export ARCH=$(uname -m) \
  && rustup target add $ARCH-unknown-linux-musl
RUN apk add --no-cache --upgrade \
  build-base \
  musl-dev \
  libpq-dev \
  openssl-dev \
  g++ \
  make \
  git \
  upx \
  perl
COPY ./bin/nanocld/migrations ./bin/nanocld/migrations
COPY ./crates/nanocl_error/src ./crates/nanocl_error/src
COPY ./crates/nanocl_stubs/src ./crates/nanocl_stubs/src
COPY ./crates/nanocl_utils/src ./crates/nanocl_utils/src
COPY ./crates/nanocld_client/src ./crates/nanocld_client/src
COPY ./bin/nanocl/src ./bin/nanocl/src
COPY ./bin/nanocl/build.rs ./bin/nanocl/build.rs
COPY ./bin/nanocld/src ./bin/nanocld/src
COPY ./bin/ncvpnkit/src ./bin/ncvpnkit/src
COPY ./bin/ncvpnkit/build.rs ./bin/ncvpnkit/build.rs
COPY ./bin/nanocld/specs ./bin/nanocld/specs
COPY ./bin/nanocld/build.rs ./bin/nanocld/build.rs
COPY ./bin/ncproxy/src ./bin/ncproxy/src
COPY ./bin/ncproxy/build.rs ./bin/ncproxy/build.rs
COPY ./bin/ncproxy/specs ./bin/ncproxy/specs
COPY ./bin/ncdns/build.rs ./bin/ncdns/build.rs
COPY ./bin/ncdns/src ./bin/ncdns/src
COPY .git ./.git
ENV NANOCL_CHANNEL=$channel
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN export ARCH=$(uname -m) \
  && cargo build --release --target=$ARCH-unknown-linux-musl --features release --bin nanocl
RUN export ARCH=$(uname -m) \
  && upx --lzma --best /app/target/$ARCH-unknown-linux-musl/release/nanocl \
  && cp /app/target/$ARCH-unknown-linux-musl/release/nanocl /bin/nanocl

# stage 4 - create runtime image
FROM --platform=$BUILDPLATFORM scratch

LABEL org.opencontainers.image.source https://github.com/next-hat/nanocl
LABEL org.opencontainers.image.description Nanocl Cli

COPY --from=builder /bin/nanocl /bin/nanocl

ENTRYPOINT ["nanocl"]
