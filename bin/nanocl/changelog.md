# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.1] - 2023-05-24

### Fixed

- `nanocl state apply -a` follow logs on wrong namespace.

## [0.7.0] - 2023-05-22

### Added

- StateFile use Kind instead of Type
- Command to restart a cargo
- Better state apply and revert templating
- StateFile now use `Kind` instead of `Type`

### Fixed

- Better handling of apply state url
- Better handling of default host from cli arguments and config
- Rename reset to revert

## [0.6.2] - 2023-05-14

### Added

- Bind Daemon config and Gateway in `StateFile.yml`
- Bind Namespaces Summary in `StateFile.yml`

## [0.6.1] - 2023-05-10

### Fixed

- `-a` option when applying a `StateFile` now.
- `Envs` are applied to the `StateFile` even if no `BuildArgs` are set.

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
