# Developing

This guide will help you to setup nanocl in development.


## Installation

Clone the repository:

```sh
git clone https://github.com/nxthat/nanocl
```

To run nanocl you will need these dependencies

- rust >= 1.67
- docker >= 1.41
- gcc
- libpq
- openssl

### Ubuntu

If you are running on ubuntu there is some scripts to help you install dependencies:

```sh
./scripts/ubuntu.deps.sh
```

If you need docker:

```sh
./scripts/install_docker.ubuntu.sh
```

### Rust

To install rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then you can install rust devtools required to run nanocl

```sh
./scripts/rust.deps.sh
```


## Running and Watching

You can run nanocl in multiple way

First you need to start a daemon, the daemon need to have right to access to docker

I personnally run the project that way:

I make sure my user is in docker group if it's not you can add it like this

```
sudo usermod -aG docker $USER
newgrp docker
```

Knowing that nanocl daemon will create a unix socket at `/run/nanocl/nanocl.sock`
I make sure the folder `/run/nanocl` exists

```sh
sudo mkdir /run/nanocl
sudo chmod 777 -R /run/nanocl
```

Before running nanocl we will need some docker images:

```sh
docker pull nexthat/metrsd:0.1.0
docker pull cockroachdb/cockroach:v22.2.6
```

Finally we can start the daemon.
You can do it in multiple way :

- Using cargo make
  ```sh
  cargo make run-daemon
  ```
- Using cargo
  ```sh
  cargo run --bin nanocld
  ```
- Using cargo watch
  ```
  cargo watch -x "run --bin nanocld"
  ```
- Using the docker compose
  ```sh
  docker compose up
  ```

I personally use the first way.

Now you can run the CLI:

- Using cargo make
  ```sh
  cargo make run-cli version
  ```
- Using cargo
  ```sh
  cargo run --bin nanocl version
  ```

## Directory structure

The project is separated into multiple crates and binaries.
We have:

- `crates/nanocl_stubs` Is shared data models between daemon client and cli.
- `crates/nanocld_client` Is a client for nanocl daemon
- `bin/nanocld` Is the nanocl daemon
- `bin/nanocl` Is the nanocl CLI
