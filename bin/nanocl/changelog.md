# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.14.0]

### Added

- Secrets create commands

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
