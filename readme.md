<div align="center">
  <img src="https://download.next-hat.com/ressources/images/logo.png" >
  <h1>Nanocl</h1>
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

[![codecov](https://codecov.io/gh/nxthat/nanocl/branch/nightly/graph/badge.svg?token=4I60HOW6HM)](https://codecov.io/gh/nxthat/nanocl)

  </p>

</div>

<blockquote>
 <span>
   Test, Deploy, Scale, Monitor, Orchestrate
 </span>
</blockquote>

## ❓ Why Nanocl

`Nanocl` is all about easing your container and VM management with Rust-powered platform.
With `Nanocl`, say goodbye to complex setups and hello to easy, efficient deployments.
We stand for robust performance and efficiency with simplicity, trimming the bloat to keep your systems lean.
**_Join us and help shape the future of cloud computing - it's about time things got a bit more rusty_**.

## 📙 Table of Contents

- [❓ Why Nanocl ?](#-why-nanocl)
- [📙 Table of Contents](#-table-of-contents)
- [🚀 Key Benefits](#-key-benefits)
- [🧿 Architecture](#-architecture)
- [📚 Documentation](#-documentation)
- [📋 Requirements](#-requirements)
- [💾 Installation](#-installation)
- [🔧 Usage](#-usage)
- [👨‍💻 Contributing](#-contributing)

## 🚀 Key Benefits

- Easy deployment and management
- Significantly reduce the delay between writing code and shipping it to production
- Efficiently manage and scale your infrastructure as your needs evolve
- Enhance security by isolating services using namespaces and networks.
- Enjoy peace of mind with automatic backups, zero downtime fail-over, and comprehensive log monitoring
- Enjoy history tracking of your container and virtual machine configuration
- Revert configuration as quickly as pressing a button
- Build an entire CI/CD pipeline, from tests to high-availability production
- Best ideas and practices from the community

## 🧿 Architecture

`Nanocl` is designed in a **micro services** architecture several component are required and they are running as **container** included the `Nanocl Daemon` itself.
The following components will be installed during `nanocl setup` and are required to ensure `Nanocl` functionnality:

- `nstore` to save our state
- `ndaemon` as **REST API** to manage everything
- `nmetrics` to monitor cpu, memory and network usage
- `nproxy` proxy to redirect traffic to our **containers** and **virtual machines**
- `ncproxy` to update proxy configuration based on the current state
- `ndns` to manage the dns entries for the **containers** and **virtual machines**
- `ncdns` to update dns entries based on the current state

Simplified version of our architecture for a single node:

<div align="center">
  <img src="./doc/architecture.png" />
</div>

## 📚 Documentation

To learn more about `Nanocl`, you can take a look at the following resources:

- [Overview](https://docs.next-hat.com/guides/nanocl/overview)
- [Get Started](https://docs.next-hat.com/guides/nanocl/get-started/orientation-and-setup)
- [CLI References](https://docs.next-hat.com/references/nanocl/cli)
- [DAEMON References](https://docs.next-hat.com/references/nanocl/daemon/overview)

## 📋 Requirements

To work properly `Nanocl` must have theses dependencies installed on the system:

- [Docker](https://www.docker.com) minimum version 1.41

## 💾 Installation

To install `Nanocl`, please refer to our online [installation guide](https://docs.next-hat.com/setups/nanocl/linux/ubuntu).

## 🔧 Usage

`Nanocl` is designed to be easy to operate by mostly using **state files**.<br />
**State Files** are `yaml` files that define the state you want.<br />
There is an example used to deploy our [documentation](https://docs.next-hat.com):

```yaml
ApiVersion: v0.8
Kind: Deployment

Namespace: nexthat

# See all options:
# https://docs.next-hat.com/references/nanocl/cargo
Cargoes:
  - Name: doc
    Container:
      Image: nexthat-doc:0.4.1

# See all options:
# https://docs.next-hat.com/references/nanocl/resource
Resources:
  - Name: docs.next-hat.com
    Kind: ProxyRule
    Version: v0.5
    Config:
      Watch:
        - doc.nexthat.c
      Rules:
        - Domain: docs.next-hat.com
          Network: Public
          Ssl:
            Certificate: /etc/letsencrypt/live/docs.next-hat.com/fullchain.pem
            CertificateKey: /etc/letsencrypt/live/docs.next-hat.com/privkey.pem
            Dhparam: /etc/letsencrypt/ssl-dhparams.pem
          Includes:
            - /etc/letsencrypt/options-ssl-nginx.conf
          Locations:
            - Path: /
              Target:
                Key: doc.nexthat.c
                Port: 80
```

To apply a state we can do it easily bu running `nanocl state apply -s path|url`<br />
We can also revert a state by calling `nanocl state rm -s path|url`

## 👨‍💻 Contributing

Every contribution is very welcome.

But to be abble to do so you need a dev environnement right ?<br />
You can learn more about it on the [contribution guide](./contributing.md).<br />
Also don't hesitate to join the discord if you have any question!
