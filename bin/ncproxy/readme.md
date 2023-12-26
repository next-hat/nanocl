<div align="center">
  <img src="https://download.next-hat.com/ressources/images/logo.png" >
  <h1>Nanocl Controller Proxy</h1>
  <p>

[![Stars](https://img.shields.io/github/stars/next-hat/nanocl?label=%E2%AD%90%20stars%20%E2%AD%90)](https://github.com/next-hat/nanocl)
[![Build With](https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat)](https://github.com/next-hat/nanocl)
[![Chat on Discord](https://img.shields.io/discord/1011267493114949693?label=chat&logo=discord&style=flat)](https://discord.gg/WV4Aac8uZg)

  </p>

  <p>

[![Tests](https://github.com/next-hat/nanocl/actions/workflows/tests.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/tests.yml)
[![Clippy](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml)

  </p>

  <p>

[![codecov](https://codecov.io/gh/next-hat/nanocl/branch/nightly/graph/badge.svg?token=RXLMUB8GA0)](https://codecov.io/gh/next-hat/nanocl)

  </p>

</div>

The official [nanocl](https://github.com/next-hat/nanocl) controller proxy build on top of nginx.

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

- Resource type `ncproxy.io/rule`:
  - Creation, Update, Suppresion
- Cargo,Vm:
  - Creation, Update, Suppression
