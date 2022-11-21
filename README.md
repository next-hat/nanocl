<div align="center">
  <img src="https://download.next-hat.com/ressources/images/logo.png" >
  <h1>Nanocl</h1>
  <h3>Hybrid Cloud Orchestrator</h3>
  <h3>CLI</h3>
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

  [![Crate.io](https://img.shields.io/crates/v/nanocl?style=flat)](https://crates.io/crates/nanocl)
  [![Github](https://img.shields.io/github/v/release/nxthat/nanocl?style=flat)](https://github.com/nxthat/nanocl/releases/latest)

  </p>

</div>

<blockquote class="tags">
 <strong>Tags</strong>
 </br>
 <span id="nxtmdoc-meta-keywords">
   Test, Deploy, Monitor, Scale, Orchestrate
 </span>
</blockquote>

## 📙 Overview

<img src="https://download.next-hat.com/ressources/images/infra.png" />

## ❓ What is nanocl ?

Nanocl, stand for `Nano Cloud` and it's a lie because you will be able to create big ones.
I see it as an open-source Hybrid Cloud Orchestrator!
To help orchestrate containers and virtual machines on multiple hosts.
It provides basic mechanisms for deployment, maintenance, and scaling.
You will be able to create your own Hybrid Cloud with optional Vpn and DNS controllers.
Deploying your applications, services or servers publicly or behind a Vpn has never been that easy!

Builds upon `Rust` to have the best performance and the smallest footprint.
It uses the best ideas and practices from the community.
You can build an entire CI/CD pipeline from tests to high-availability production.
See it as a Kubernetes alternative with more features and a network security layer.

This repository is the CLI version you can see the DAEMON [here](https://github.com/nxthat/nanocld).

## ✨ Features
- [x] Manage clusters (CRUD)
- [x] Manage networks (CRUD)
- [x] Manage containers (CRUD)
- [X] Manage DNS entries
- [x] Http proxy
- [x] Udp/Tcp proxy
- [x] Monitor http request
- [x] Single-node mode
- [x] Store a git repository state as image
- [ ] Manage VPN
- [ ] Highly-scalable distributed node mode
- [ ] Manage virtual machine (CRUD)
- [ ] Monitor tcp/udp packets

## 🎉 Let's get started

- [Installation](https://docs.next-hat.com/docs/setups/nanocl)
- [Tutorial](https://docs.next-hat.com/docs/guides/nanocl/get-started)

## 🔨 Contribution

If you want to contribute see how to [build from source](https://docs.next-hat.com/docs/setups/nanocl/linux/from-sources)
section on our official documentation to see how to setup a environnement for nanocl
