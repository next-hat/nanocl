# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [untagged]

### Added

- Cargo scale endpoint on `PATCH /cargoes/{Name}/scale` to scale up or down a Cargo.

## [0.8.0] - 2023-06-03

### Changed

- Cargo instances handling with `FutureUnordored`
- Better state apply and remove with `FutureUnordored` by [@cszach](https://github.com/cszach)
- Resource `Custom` renamed to `Kind` and allow custom url for hook by [@CreepyPvP](https://github.com/CreepyPvP)

### Fixed

- Unix socket permission when rebooting

## [0.7.0] - 2023-05-22

### Changed

- Vm runtime image to `ghcr.io/nxthat/nanocl-qemu:7.1.0.0`
- Better state messages

### Added

- Endpoint to restart a cargo
- Statefile use Kind instead of Type
- Replication `Static` that can allow development tests

### Fixed

- Rename reset to revert
- Missing `created_at` field for resources
- Order by `created_at` by default for resources

## [0.6.1] - 2023-05-14

### Fixed

- Patch a cargo when he is restarting

## [0.6.0] - 2023-05-05

### Added

- Add latest version in openapi
- HEAD /\_ping method from computed version in url
- GET /version method from computed verion in url
- Metrics listing by `kind`
- Namespace filter by ilike `%Name%` and by `limit` and `offset`
- Open Cors policy
- OPTIONS endpoints for browser compatibility with Cors
- State apply and revert return a stream
- Option to force remove a cargo by [@CreepyPvP](https://github.com/CreepyPvP)
- Cargo logs options `follow` | `tail` | `until` | `since` | `timestamps` by [@CreepyPvP](https://github.com/CreepyPvP)
- Cargo filter by ilike `%Name%` and by `limit` and `offset` by [@CreepyPvP](https://github.com/CreepyPvP)
- Better controller definition with version
- GET /info now return information about daemon configuration

### Fixed

- Cargo logs return type if `stream`
- Image name

### Refactor

- Better error handling

## [0.5.0] - 2023-04-15

### Added

- Namespace network information
- Upgrade ncdproxy to 0.3
- Upgrade nproxy to 1.23.4.0
