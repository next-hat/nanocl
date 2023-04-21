# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0]

### Added

- Add latest version in openapi
- HEAD /_ping method from computed version in url
- GET /version method from computed verion in url
- Metrics listing by `kind`
- Namespace filter by like `%Name%` and by `limit` and `offet`
- Open Cors policy
- OPTIONS endpoints for browser compatibility with Cors
- State apply and revert return a stream
- Option to force remove a cargo by [@CreepyPvP](https://github.com/CreepyPvP)

### Fixed

- Cargo logs return type if `stream`

## [0.5.0] - 2023-04-15

### Added

- Namespace network information
- Upgrade ncdproxy to 0.3
- Upgrade nproxy to 1.23.4.0
