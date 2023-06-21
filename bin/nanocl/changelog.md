# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [untagged]

### Added

- Acceptance of `VirtualMachine` type for state files by [@tyrone-wu](https://github.com/tyrone-wu)
- Option `-kvm` when running or creating a VM

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
- `Envs` are applied to the `Statefile` even if no `BuildArgs` are set.

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
