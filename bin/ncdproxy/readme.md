<div align="center">
  <img src="https://download.next-hat.com/ressources/images/logo.png" >
  <h1>Nanocl Controller Proxy</h1>
  <p>

  [![Stars](https://img.shields.io/github/stars/nxthat/nanocl?label=%E2%AD%90%20stars%20%E2%AD%90)](https://github.com/nxthat/nanocl)
  [![Build With](https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat)](https://github.com/nxthat/nanocl)
  [![Chat on Discord](https://img.shields.io/discord/1011267493114949693?label=chat&logo=discord&style=flat)](https://discord.gg/WV4Aac8uZg)

  </p>

  <p>

  [![Tests](https://github.com/nxthat/nanocl/actions/workflows/tests.yml/badge.svg)](https://github.com/nxthat/nanocl/actions/workflows/tests.yml)
  [![Clippy](https://github.com/nxthat/nanocl/actions/workflows/clippy.yml/badge.svg)](https://github.com/nxthat/nanocl/actions/workflows/clippy.yml)

  </p>

  <p>

  [![codecov](https://codecov.io/gh/nxthat/nanocl/branch/nightly/graph/badge.svg?token=RXLMUB8GA0)](https://codecov.io/gh/nxthat/nanocl)

  </p>

</div>

The official [nanocl](https://github.com/nxthat/nanocl) controller proxy build on top of nginx.

This microservice watch event sent by the nanocl daemon to generate nginx config file to enable exposition of your cargo.

## Architecture

```mermaid
sequenceDiagram
  participant Daemon
  participant Ctrl Proxy
  participant Proxy
  critical Establish a connection to the Daemon
    Ctrl Proxy->>Daemon: connect
  option Connection fail
    loop Every 2 seconds
      Ctrl Proxy->>Daemon: Reconnect
    end
  option Connection success
    loop On events
      Daemon-)Ctrl Proxy: Send event
      Ctrl Proxy-)Proxy: Update configuration
    end
  end
```

The controller proxy will watch for events:
- Resource type `ProxyRule`:
  * Creation, Update, Suppresion
- Cargo:
  * Creation, Update, Suppression

## Installation


### Production

You need to have nanocl installed on your system, see how to install in our [documentation](https://docs.next-hat.com/setups/nanocl/).

Then you need to download our images:

```sh
docker pull ghcr.io/nxthat/nanocl-proxy:latest
docker pull ghcr.io/nxthat/ncdproxy:latest
```

### Development

You still need to have nanocl installed on your system.

Build dev and test image

```sh
docker build -t nanocl-proxy:test -f ./nginx/Dockerfile .
docker build -t nanocl-proxy:dev -f ./nginx/Dockerfile .
docker build -t nanocl-ncdproxy:dev -f dev.Dockerfile .
```

Apply nanocl dev state

```
nanocl state apply -af ./nanocl/dev.yml
```
