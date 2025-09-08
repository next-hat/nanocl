<div align="center">
  <img width="142" height="142" src="https://download.next-hat.com/ressources/images/logo.png" >
  <h2>Nanocl</h2>
  <h4>Just Develop, Deploy.</h4>
  <h6>Orchestrate like never before. K8s reimagined.</h6>
  <p align="center">
    <a href="https://next-hat.com/nanocl"><b>Why</b></a> •
    <a href="https://docs.next-hat.com/manuals/nanocl/install/overview"><b>Install</b></a> •
    <a href="https://discord.gg/WV4Aac8uZg" target="_blank"><b>Discord</b></a> •
    <a href="https://x.com/next_hat" target="_blank"><b>𝕏</b></a>
  </p>
  <p>

[![Tests](https://github.com/next-hat/nanocl/actions/workflows/tests.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/tests.yml)
[![Clippy](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml/badge.svg)](https://github.com/next-hat/nanocl/actions/workflows/clippy.yml)
[![codecov](https://codecov.io/gh/next-hat/nanocl/branch/nightly/graph/badge.svg?token=4I60HOW6HM)](https://codecov.io/gh/next-hat/nanocl)

  </p>
</div>

> Quick install (requires Docker)

```bash
curl -fsSL https://download.next-hat.com/scripts/get-nanocl.sh | sh
```

Then complete the required [Post‑installation steps][post_install].

**Nanocl** is an open‑source distributed system that reimagines **cloud‑native orchestration** from the ground up.

It gives developers a frictionless **local to production** workflow while hiding multi‑service complexity (no more ad‑hoc scripts, CORS hacks, or brittle docker‑compose sprawl). The same declarative model powers both dev and prod.

> You could rebuild a K8s inside Nanocl… but you probably won't want to.

Built in 🦀 **Rust** for efficiency & security, Nanocl targets **platform engineers** who need strong isolation, predictable performance, and simpler operations—without the cognitive overhead of a full Kubernetes distribution.

## Table of Contents

1. [Why Nanocl](#why-nanocl)
2. [Key Features](#key-features)
3. [Installation](#installation)
4. [Quick Start](#quick-start)
5. [Latest news](#latest-news)
6. [Usage](#usage)
7. [Architecture](#architecture)
8. [Demo](#demo)
9. [Roadmap](#roadmap)
10. [Security](#security)
11. [Contribute](#contribute)
12. [Support & Community](#support--community)
13. [Sponsors](#sponsors)
14. [License](#license)
15. [Star History](#star-history)

## Why Nanocl

Modern teams need faster iteration loops, secure multi‑tenant isolation, and a production‑grade transition path—without operating a heavyweight control plane on day one. Nanocl focuses on:

- Dev→Prod symmetry: declarative Statefiles (YAML/TOML/JSON) stay identical across environments.
- Low operational overhead: minimal moving parts; opinionated defaults.
- Security & efficiency: memory‑safe implementation, reduced footprint versus scripting stacks.
- Extensibility: resources & rules let you model networking, routes, TLS, jobs, and more.
- Pragmatic scope: not a Kubernetes clone—lean where K8s is heavy, expressive where docker‑compose is limiting.

## Key Features

- Declarative Statefiles for cargoes (containers), resources, jobs & virtual machines.
- End‑to‑end TLS (including internal mesh primitives in progress).
- Dynamic routing & DNS propagation (`nproxy`, `ncproxy`, `ndns`).
- Resource abstraction layer for pluggable kinds (e.g. proxy rules).
- Jobs & cron style automation.
- Backup & orphan cleanup tooling.
- Multi-format config: YAML / TOML / JSON.
- Batteries included CLI & daemon.
- Minimal host footprint; modular optional services.

## Installation

Nanocl supports **Linux**, **macOS**, and **Windows**. See the [Installation guide][nanocl_install_guide].

### Quick install

Run the installer (requires Docker):

```bash
curl -fsSL https://download.next-hat.com/scripts/get-nanocl.sh | sh
```

Then complete the required [Post‑installation steps][post_install].

If you're just evaluating, a single-node install usually takes under two minutes.

## Quick Start

Create a minimal Statefile and apply it:

```yaml
ApiVersion: v0.16
Cargoes:
  - Name: hello
    Container:
      Image: ghcr.io/next-hat/documentation:0.16.0
```

Apply & inspect:

```bash
nanocl state apply -s ./state.yml
nanocl cargo ls
nanocl cargo logs hello
```

Remove it:

```bash
nanocl state rm -s ./state.yml
```

Next: explore [Get Started][nanocl_get_started].

> Nanocl is evolving fast. Feedback & early adopters shape the roadmap—see below.

## Latest news

- **Blog**: [Automating deployment with GitHub Actions](https://docs.next-hat.com/blog/automating-deployment-with-github-actions-and-nanocl) on 24.11.2024
- **Release**: [End to End TLS encryption and first step for network meshing](https://docs.next-hat.com/blog/nanocl-0.16) on 01.11.2024
- **Release**: [Man page, Backup, Remove Orphans and more](https://docs.next-hat.com/blog/nanocl-0.15) on 11.06.2024
- **Event**: [We are invited to the Merge Berlin 2024](https://www.linkedin.com/feed/update/urn:li:activity:7201921660289998850) on 01.06.2024
- **Release**: [Context, SubState and more](https://docs.next-hat.com/blog/nanocl-0.14) on 07.05.2024

## Usage

Statefiles drive everything. Here's the Statefile we use to deploy our own [documentation][documentation]:

```yaml
ApiVersion: v0.16
Cargoes:
  - Name: doc
    Container:
      Image: ghcr.io/next-hat/documentation:0.16.0
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

Actions:

- Apply: `nanocl state apply -s ./state.yml`
- Remove: `nanocl state rm -s ./state.yml`

More object options in the [References][nanocl_daemon_ref].

## Architecture

Nanocl is composed of modular containerized services:

| Component | Purpose |
|-----------|---------|
| nstore | Persist cluster state |
| ndaemon | REST API & control surface |
| nmetrics | Resource usage collection |
| nproxy | Data‑plane proxy for containers/VMs (optional) |
| ncproxy | Reconciles proxy config (optional) |
| ndns | DNS records manager (optional) |
| ncdns | DNS reconciliation (optional) |

Resources:

- [Overview][nanocl_overview]
- [Get Started][nanocl_get_started]
- [CLI References][nanocl_cli_ref]
- [Daemon References][nanocl_daemon_ref]

Single‑node simplified architecture:

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

## Roadmap

High‑level focus (non‑exhaustive):

- Service mesh primitives (progressive rollout of secure networking).
- Extended resource kinds (cert management, quotas).
- Multi‑node clustering UX refinements.
- Improved observability (tracing & richer metrics).
- Provider abstraction layer (future pluggable runtimes).

Track changes & releases in the [blog](https://docs.next-hat.com/blog/).

## Security

Please report vulnerabilities via our [Security Policy](./SECURITY.md). Responsible disclosure helps everyone.

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

## Support & Community

- Chat: [Discord][discord]
- Issues: GitHub Issues & Discussions
- Questions: Open a discussion or join Discord
- Updates: Follow us on 𝕏

## Sponsors

<blockquote>
 <span>
    GitHub ⭐️ helps us a lot to further grow our open-source ecosystem for & with our community.
 </span>
</blockquote>

Sponsors are **the ones who make this project possible**.<br/>
They help us to have the necessary resources for Nanocl to keep it alive and to improve it further.<br/>
If you want to **become a sponsor**, please use the GitHub Sponsor button.<br/>

People that sponsor us will have their **name** or **logo displayed here**, and will have access to a **special role** on our *[Discord][discord]*.

**Our very kind sponsors:**

<table>
  <tr>
    <td align="center">
      <a href="https://github.com/daemonfire300">
        <img src="https://images.weserv.nl/?url=avatars.githubusercontent.com/u/135746?v=4&h=300&w=300&fit=cover&mask=circle&maxage=7d" width="100" alt="daemonfire300" />
        <br/>
        <sub>
          <b>
            daemonfire300
          </b>
        </sub>
      </a>
    </td>
  </tr>
</table>

## License

Dual licensed under either:

- [Apache License, Version 2.0](./LICENSE-APACHE)
- [MIT License](./LICENSE-MIT)

You may use either license at your discretion.

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
[post_install]: https://docs.next-hat.com/manuals/nanocl/install/post-installation
