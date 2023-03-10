# Nanocld daemon development image
FROM rust:1.68.0-alpine3.17

## Setup cargo-watch
RUN apk add gcc g++ make
RUN cargo install cargo-watch

## Install dependencies
RUN apk add libpq-dev util-linux tzdata git util-linux inotify-tools bash

## Setup the project
RUN mkdir -p /project
WORKDIR /project

## Setup the environment
ENV TZ=Europe/Paris
ENV RUSTFLAGS="-C target-feature=-crt-static"

COPY ./bin/nanocld/entrypoint.dev.sh /entrypoint.sh

RUN chmod +x /entrypoint.sh

## Set entrypoint
ENTRYPOINT ["/bin/bash", "/entrypoint.sh"]
