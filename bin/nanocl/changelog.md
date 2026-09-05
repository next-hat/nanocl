# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.18.0] - 2026-09-05

### Core

- Upgrade dependencies

### Added

- Process-scoped `nanocl exec` and `nanocl kill` commands for targeting one
  concrete container by process name or Docker ID.
- Cargo health status and named application containers in command output.
- Start and delete Cargo instances on a specific node.

### Changed

- **Breaking:** Cargo Statefiles now use named `Containers` and ordered
  `InitContainers`, an integer `Replicas` field, and Cargo-owned network, port,
  hostname, and DNS settings. The previous single-container fields and
  replication modes are no longer accepted.
- **Breaking:** Cargo and VM commands now identify namespaced resources with
  canonical `{namespace}.{name}` keys; list commands can span namespaces.
- `nanocl cargo patch` accepts `--container` to select the application container
  to update and requires it when a Cargo has multiple application containers.
- VM Statefiles now use a full local `Image` path and an optional
  `InitContainer`; VM attach uses the process-scoped daemon endpoint.
- Statefile rendering and argument handling support required, multiple, described and default values.

### Removed

- Cargo-scoped exec and group-wide kill operations; use process-scoped
  `nanocl exec` and `nanocl kill` instead.
- The `--rm` option from `nanocl cargo run`.
- The `nanocl vm image` command and daemon-managed VM image workflow.

### Fixed

- Select CockroachDB v23.1.30 on x86-64 hosts without the complete x86-64-v3
  CPU and OS feature set.
- Job list and inspection output now reads status and timestamps from their
  corrected top-level API fields.
- `nanocl backup` now strips the nanocld auto-mounted secret bind (`/opt/nanocl.io/secrets`) from cargoes and jobs so backups can be reapplied without manual edits.

## [0.17.1] - 2025-09-17

### Fixed

- Fix substate default arguments population when rendering a statefile.

## [0.17.0] - 2025-09-12

### Added

- `nanocl stats` to get stats of multiple processes by name [n0tank3sh](https://github.com/n0tank3sh)
- `nanocl state man` to get the manual of the Statefile.
- `nanocl state render` to render a Statefile with the current context variables.
- `nanocl logs` now accept multiple names.
- Better statefile arguments parsing with required, multiple and description fields.
- Statefile metadata to enhance the manual of a statefile.
- `nanocl secret patch` to update a secret without having to remove it.

## [0.16.3] - 2025-03-14

### Fixed

- Fix `nanocl state rm` error handling.

## [0.16.2] - 2024-11-24

### Changed

- Use of ntex-util 2.6.1 to remove debug print

## [0.16.1] - 2024-11-23

### Fixed

- SSL/TLS certificate verification
- Host ssl configuration when applying a state

## [0.16.0] - 2024-11-01

### Added

- `nanocl logs` to logs a single process
- `nanocl inspect` to inspect a single process

### Changed

- Use of nanocld_client 0.16.0

### Fixed

- Windows compilation and compatibility

## [0.15.0] - 2024-06-11

### Added

- Status information in the table of cargo ls and vm ls command and job ls
- `nanocl metric inspect` command to get details about a metric
- `nanocl event inspect` command to get details about an event
- `nanocl backup` command to backup the current state into multiple Statefiles
- `HOST` env variable to override the default host
- `CERT` and `CERT_KEY` env variable to pass certificate and certificate key to the client
- `nanocl state apply --remove-orphans` to remove orphaned objects
- `nanocl secret create tls` now accept path to the certificate and key

### Fixed

- `nanocl cargo run` now correctly wait the cargo to be running before returning
- `nanocl vm run` now correctly wait the vm to be running before returning
- `nanocl cargo start` now correctly wait the cargo to be running before returning
- `nanocl cargo stop` now correctly wait the cargo to be stopped before returning
- `nanocl cargo patch` now correctly wait the cargo to be patched before returning
- `nanocl job start` now correctly wait the job to be running before returning
- `nanocl vm start` now correctly wait the vm to be running before returning
- `nanocl vm stop` now correctly wait the vm to be stopped before returning
- Diff trigger when applying a Statefile now correctly compare the current state with the new state

### Changed

- `inspect` `rm` `stop` `start` have been refactored to have a single interface matching all object
- Removed the namespace in the table of cargo ls and vm ls command
- Cleaner Loader when apply and removing Statefile

## [0.14.0] - 2024-05-08

### Added

- Secrets create commands
- Contexts, to change the default endpoint from the default unix:///run/nanocl.sock
- Include partial Statefiles by url, relative or absolute paths
- SubStates in Statefiles to include other Statefiles

### Fixed

- nanocl state logs for jobs

### Changed

- PS default display only running processes
- State command with event system

### Added
- PS options all

## [0.13.0] - 2023-12-28

### Changed

- State apply and remove with new loader and logic.
- Install and uninstall with new loader and logic.
- Use of nanocld_client 0.13.0

### Added

- PS command with filter by kind, namespace, limit and offset.

### Fixed

- Fixed missing openssl

## [0.12.0] 2023-12-22

### Added

- Nanocl state apply return exit code on errors.
- Nanocl state apply --reload to skip diff check.
- Cargo image import with progress bar.
- Fix double create_at column in `nanocl cargo ls`.
- Better `nanocl ps`.
- Install command with `-p | --force-pull` to force repull image

### Changed

- Use of nanocld_client 0.12.

## [0.11.0] - 2023-11-06

### Added

- Download InitContainer image when running state apply
- Use of nanocld_client v0.11.0

## [0.10.0] - 2023-10-01

### Added

- Options for cargo exec: tty, detach_keys, env, privileged, user, working_dir [@anonkey](https://github.com/anonkey)
- return executed command status code from cargo exec [@anonkey](https://github.com/anonkey)
- Arguments Number and Boolean for Statefile
- Os,OsFamilly and Context inside the Statefile templating variable
- Cargo stats command
- State logs command
- Secret management

### Changed

- Use of nanocld_client v0.10.0 (exec_cargo becomes create_exec and start_exec) [@anonkey](https://github.com/anonkey)

## [0.9.0] - 2023-07-04

### Added

- Acceptance of `VirtualMachine` type for state files by [@tyrone-wu](https://github.com/tyrone-wu)
- Option `-kvm` when running or creating a VM
- Vm start,stop,remove take an array of name
- Vm run `-a` options to attach to the vm directly after the run
- Docker desktop compatible installation
- Accept `.toml` and `.json` along side `.yml`
- Context to manage multiple nanocl host
- `--kvm` options when patching a virtual machine

### Changed

- New state apply and remove UI

### Fixed

- Default installer url
- Docker desktop host

## [0.8.1] - 2023-06-04

### Added

- Dotenv to configure env variable from a `.env`

### Fixed

- Ctrl+C wasn't existing the program correctly when following logs

## [0.8.0] - 2023-06-03

### Changed

- `nanocl state revert` is now `nanocl state remove`
- `nanocl state apply` and `nanocl state remove` use options `-s` instead of `-f` to specify the file or url.
- `nanocl state apply -f` now follow logs of created cargoes.

### Added

- `nanocl state apply -p` to force repull container image.
- Quiet option `-q` on list operation to only print name,id or key.

### Fixed

- `nanocl state apply -a` follow correctly replicat.

## [0.7.1] - 2023-05-24

### Fixed

- `nanocl state apply -a` follow logs on wrong namespace.

## [0.7.0] - 2023-05-22

### Added

- Statefile use Kind instead of Type
- Command to restart a cargo
- Better state apply and revert templating
- Statefile now use `Kind` instead of `Type`

### Fixed

- Better handling of apply state url
- Better handling of default host from cli arguments and config
- Rename reset to revert

## [0.6.2] - 2023-05-14

### Added

- Bind Daemon config and Gateway in `Statefile.yml`
- Bind Namespaces Summary in `Statefile.yml`

## [0.6.1] - 2023-05-10

### Fixed

- `-a` option when applying a `Statefile` now.
- `Envs` are applied to the `Statefile` even if no `BuildArg` are set.

## [0.6.0] - 2023-04-30

### Fixed

- Installer wasn't creating the network required for nanocl components

### Added

- Option to force remove a cargo by [@CreepyPvP](https://github.com/CreepyPvP)
- Option `follow`, `tail`, `timestamp` for cargo logs by [@CreepyPvP](https://github.com/CreepyPvP)
- Install command
- Uninstall command
- Upgrade command
- Installer fetch template from our official repo or can take custom template path
