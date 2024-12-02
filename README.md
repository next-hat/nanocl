<div align="center">
  <img width="142" height="142" src="https://download.next-hat.com/ressources/images/logo.png" >
  <h2>Nanocl</h2>
  <h4>Just Develop, Deploy.</h4>
  <h6>Orchestrate like never before. K8s reimagined.</h6>
  <p align="center">
    <a href="https://next-hat.com/nanocl"><b>Why</b></a> •
    <a href="https://docs.next-hat.com/manuals/nanocl/install/overview"><b>Quickstart</b></a> •
    <a href="https://x.com/next_hat" target="_blank"><b>𝕏</b></a>
  </p>
  <p>

[![Tests](https://github.com/next-hat/nanocl/actions/workflows/tests.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/tests.yml)
[![Clippy](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml)
[![codecov](https://codecov.io/gh/next-hat/nanocl/branch/nightly/graph/badge.svg?token=4I60HOW6HM)](https://codecov.io/gh/next-hat/nanocl)
[![Discord](https://img.shields.io/badge/discord-join-7289DA.svg?logo=discord&longCache=true&style=flat)](https://discord.gg/WV4Aac8uZg)

  </p>
</div>

**Nanocl** is an open source distributed system designed to revolutionize **cloud native** from the ground up.

The developing ecosystem provides an comprehensive solution for **localhost** development headache, avoiding common issues like CORS & cookies when working with **complex microservices**. It **reduces** infrastructure **complexity** while delivering **seamless deployment** to **production** among other innovations.

<blockquote>
 <span>
    In a nutshell one could say we're solving known K8s memes.
 </span>
</blockquote>

Nanocl will introduce the **next paradigm** for **Platform Engineers** by providing ground breaking cloud-native architectural innovations, best-in-class security, while reducing operational costs with e.g. 🦀 **Rust** based efficiency.

## Quickstart

We are already compatible with **Linux**, **MacOS** & **Windows**, just jump to our [Installation guide][nanocl_install_guide].<br/>
Please bare with us, Nanocl is currently more than just on the cutting edge..

## Latest news

<blockquote>
 <span>
    Who said that K8s is more than a perfect platform for application workloads?
 </span>
</blockquote>

- **Blog**: [Automating deployment with GitHub Actions](https://docs.next-hat.com/blog/automating-deployment-with-github-actions-and-nanocl) on 24.11.2024
- **Release**: [End to End TLS encryption and first step for network meshing](https://docs.next-hat.com/blog/nanocl-0.16) on 01.11.2024 
- **Release**: [Man page, Backup, Remove Orphans and more](https://docs.next-hat.com/blog/nanocl-0.15) on 11.06.2024
- **Event**: [We are invited to the Merge Berlin 2024](https://www.linkedin.com/feed/update/urn:li:activity:7201921660289998850) on 01.06.2024
- **Release**: [Context, SubState and more](https://docs.next-hat.com/blog/nanocl-0.14) on 07.05.2024

## Usage

Nanocl is designed to be **easy** to setup, use & maintain by mostly using **Statefiles** (`yaml`, `toml` or `json`).<br/>
Below is an **example** which is used to **deploy** our own [Documentation][documentation].

• `Apply` a state to the cluster via `nanocl state apply -s path|url`<br/>
• `Remove` it by executing `nanocl state rm -s path|url`<br/>

```yaml
ApiVersion: v0.16

# Options: https://docs.next-hat.com/references/nanocl/objects/cargo
Cargoes:
- Name: doc
  Container:
    Image: ghcr.io/next-hat/documentation:0.16.0

# Options: https://docs.next-hat.com/references/nanocl/objects/resource
Resources:
- Name: docs.next-hat.com
  Kind: ncproxy.io/rule
  Data:
    Rules:
    - Domain: docs.next-hat.com
      Network: Public
      Locations:
      - Path: /
        Target:
          Key: doc.global.c
          Port: 80
```

## Architecture

<blockquote>
 <span>
    You could build a K8s within Nanocl. But we are quite sure you wouldn't want to..
 </span>
</blockquote>

Nanocl is designed as a **microservice** architecture, consisting of multiple components running as **containers**, including the **Nanocl Daemon** itself.
The following components will be installed during `nanocl install` and are required to ensure full Nanocl functionalities:

- `nstore` to **save** cluster **state**
- `ndaemon` as **REST API** to manage everything
- `nmetrics` to monitor CPU, Memory and Network usage
- `nproxy` proxy to redirect traffic to our **containers** and **virtual machines** (optional)
- `ncproxy` to update proxy configuration based on the current state (optional)
- `ndns` to manage the dns entries for the **containers** and **virtual machines** (optional)
- `ncdns` to update dns entries based on the current state (optional)

To learn more about Nanocl, take a look at the following resources:

- [Overview][nanocl_overview]
- [Get Started][nanocl_get_started]
- [CLI References][nanocl_cli_ref]
- [Daemon References][nanocl_daemon_ref]

Simplified version of Nanocl architecture for a single node:

<div align="center">
  <img src="./doc/architecture.png" />
</div>

## Demo

#### Cargo & Resource

<div align="center">
  <img src="./doc/cargo_resource_example.gif" />
</div>

#### Job

<div align="center">
  <img src="./doc/job_example.gif" />
</div>

#### VM

<div align="center">
  <img src="./doc/vm_example.gif" />
</div>

## Contribute

<blockquote>
 <span>
  Little by little, a little becomes a lot.
 </span>
</blockquote>

Join our *[Discord][discord]* the be part of *[NextHat][next_hat]*'s journey to **shape** the **future** of **planet-scale infrastructure management**.

**Every contribution is welcomed**.<br/>
➡️ Bug reports, feature requests, and pull requests are the most common ways to contribute.<br/>
For example if you're not a developer yourself you could help us by improving the [Documentation][documentation_repository], too.

Learn how to **setup** a **development environment** via the [Contribution Guide][contributing_guide].<br/>
Please don't hesitate to **join our team** on [Discord][discord] if you have any questions! 🤗

### Sponsors

<blockquote>
 <span>
    GitHub ⭐️ helps us a lot to further grow our open-source ecosystem for & with our community.
 </span>
</blockquote>

Sponsors are **the ones who make this project possible**.<br/>
They help us to have the necessary resources for Nanocl, so we're able to keep it alive & of course to improve it further.<br/>
If you want to **become a sponsor**, please use the GitHub Sponsor button.<br/>

People that sponsor us will have their **name** or **logo displayed here**, and will have access to a **special role** on our *[Discord][discord]*.<br/><br/>

<p align="center">
  <b>Our very kind sponsors</b>
</p>

<table align="center">
  <tr>
    <td align="center">
      <a href="https://github.com/mamaicode">
        <img src="https://images.weserv.nl/?url=avatars.githubusercontent.com/u/102310764?v=4&h=300&w=300&fit=cover&mask=circle&maxage=7d" width="100" alt="mamaicode" />
        <br/>
        <sub>
          <b>
            mamaicode
          </b>
        </sub>
      </a>
    </td>
    <td align="center">
      <a href="https://github.com/xf10w">
        <img src="https://images.weserv.nl/?url=avatars.githubusercontent.com/u/43791027?v=4&h=300&w=300&fit=cover&mask=circle&maxage=7d" width="100" alt="xf10w" />
        <br/>
        <sub>
          <b>
            xf10w
          </b>
        </sub>
      </a>
    </td>
        <td align="center">
      <a href="https://github.com/xf10w">
        <img src="https://images.weserv.nl/?url=avatars.githubusercontent.com/u/142700635?v=4&h=300&w=300&fit=cover&mask=circle&maxage=7d" width="100" alt="Rembo1510" />
        <br/>
        <sub>
          <b>
            Rembo1510
          </b>
        </sub>
      </a>
    </td>
  </tr>
</table>
<br/>

## Star History

<blockquote>
 <span>
    We are just at the beginning of a new paradigm shift..
 </span>
</blockquote>

[![Star History Chart](https://api.star-history.com/svg?repos=next-hat/nanocl&type=Date)](https://star-history.com/#next-hat/nanocl&Date)

[contributing_guide]: ./CONTRIBUTING.md
[next_hat]: https://next-hat.com
[documentation]: https://docs.next-hat.com
[nanocl_overview]: https://docs.next-hat.com/guides/nanocl/overview
[nanocl_install_guide]: https://docs.next-hat.com/manuals/nanocl/install/overview
[nanocl_get_started]: https://docs.next-hat.com/guides/nanocl/get-started/orientation-and-setup
[nanocl_cli_ref]: https://docs.next-hat.com/references/nanocl/cli
[nanocl_daemon_ref]: https://docs.next-hat.com/references/nanocl/daemon/overview
[docker]: https://www.docker.com
[discord]: https://discord.gg/WV4Aac8uZg
[documentation_repository]: https://github.com/next-hat/documentation
