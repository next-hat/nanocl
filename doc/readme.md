# Nanocl developer documentation

Step in a unknow project can be dificult, even when you have some experiences.
This documentation will help you to setup

## 📙 Table of Contents

* [📁 Project Structure](#-project-structure)


## Project Structure

`Nanocl` is using a **mono repository structure**.<br />

```sh
bin
├── ctrl-dns # Controller DNS
│   ├── dnsmasq # Source to build dnsmasq container image
│   └── src # Rust source code
├── ctrl-proxy # Controller PROXY
│   ├── nginx # Source to build nginx container image
│   │   └── html
│   ├── src # Rust source code
│   └── tests # Configuration to tests
├── nanocl # Nanocl CLI
│   └── src # Rust source code
│       ├── commands # Function that execute commands
│       ├── models # Data structure used in the project
│       └── utils # Utils functions
└── nanocld # Nanocl DAEMON REST API
    ├── migrations # Container SQL migration generated with diesel
    │   ├── 00000000000000_diesel_initial_setup
    │   ├── 2022-05-20-134629_create_namespaces
    │   ├── 2022-06-17-122356_create_cargos
    │   ├── 2022-08-04-214925_create_nodes
    │   ├── 2023-01-15-121652_resources
    │   ├── 2023-02-17-193350_metrics
    │   └── 2023-03-10-234850_vms
    ├── specs # Configuration the daemon will apply at runtime
    │   └── controllers # Controller configurations the daemon will apply at runtime
    └── src # Rust source code
        ├── models # Data structure used in the project
        ├── repositories # Functions to make SQL request
        ├── services # Function to accepts http request
        ├── subsystem # Function every runtime to ensude the default state is setup
        └── utils # Utils functions
crates # Libraries
├── nanocld_client # A nanocld client
│   └── src # The rust source code
└── nanocl_stubs # Shared data structure mostly used as input and output of out DAEMON
    └── src # The rust source code
```
