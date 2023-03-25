# Create Builder image
FROM rust:1.68.0-alpine3.17

ARG tz=Europe/Paris

# Install required dependencies
RUN apk add openssl
RUN apk add libpq-dev
RUN apk add gcc
RUN apk add g++
RUN apk add make
RUN apk add tzdata
RUN apk add util-linux
RUN cargo install cargo-watch

RUN mkdir -p /project
WORKDIR /project

ENV TZ=${tz}

ENTRYPOINT ["cargo", "watch", "-x", "run --bin ctrl-dns"]
