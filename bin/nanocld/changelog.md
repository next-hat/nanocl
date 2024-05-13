# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.15.0]

### Added

- `--store-addr` options from the command line to specify the store address
- Endpoint `GET /metrics/{key}/inspect` to get details about a metric
- Endpoint `GET /event/{key}/inspect` to get details about an event

### Fixed

- Removing a job with a running process will now stop the process before removing the job

### Changed

- The path to inspect a resource `GET /resources/{name}` is now `GET /resources/{name}/inspect`

## [0.14.0] - 2024-05-08

### Changed

- New event system
- Download image in background
- Queu task for process object (job, cargo, vm)
- Object process status
- ImagePullPolicy option to always pull the image or only if it doesn't exists
- ImagePullSecret option for private registry

### Fixed

- Send correct event when updating a secret and a resource
- Replication feature

## [0.13.0] - 2023-12-28

### Changed

- Removed state apply and remove endpoints.

### Fixed

- Secret creation when using native kind.
- Resource kind was saved with it's version.
- Openssl linking.

### Added

- Processes with generic filter from the endpoint.


## [0.12.0] - 2023-12-22

### Added

- SSL/TLS connection to the store (db).
- StateApplyQuery with reload option to skip diff.
- Job to run a command as container.
- Job schedule to run job at scheduler interval like a cron.
- Process to save container state and reduce repeated code.
- Generic filter on find and find_one for all models.
- Better versionning middleware that return the version.
- Cleaner code and data structure in general
- Stabilize spec with modification on Kind and Version handling and history of object

## Changed

- Event datastructure

## [0.11.0] 2023-11-06

### Added

- Filter by kind, existing key and contains for data and metadata of a secret. [@anonkey](https://github.com/anonkey)
- Filter by existing key and contains for data and metadata of a resource.
- InitContainer to Cargo, to run a command before creating the cargo.

## [0.10.0] - 2023-10-04

### Changed

- split exec_cargo in create_exec start_exec [@anonkey](https://github.com/anonkey)

### Added

- inspect_exec endpoint [@anonkey](https://github.com/anonkey)
- stats_cargo endpoint
- metadata attribute for vm cargo and resource
- secret model for sensitive data, env variable or ssl certificate

### Removed

- exec_cargo endpoint [@anonkey](https://github.com/anonkey)

## [0.9.1] - 2023-07-15

### Added

- Log tcp/udp request by [@Narayanbhat166](https://github.com/Narayanbhat166)

### Fixed

- Virtual image sync naming

## [0.9.0] - 2023-07-04

### Added

- Cargo scale endpoint on `PATCH /cargoes/{Name}/scale` to scale up or down a Cargo.
- Sync VM image directory to nanocld system by [@tyrone-wu](https://github.com/tyrone-wu)
- Acceptance of `VirtualMachine` type for state files by [@tyrone-wu](https://github.com/tyrone-wu)

### Fixed

- VM runtime with default to latest `nanocl-qemu` image
- Removed useless devices bindings to start a VM

### Changed

- State apply and remove event

## [0.8.0] - 2023-06-03

### Changed

- Cargo instances handling with `FutureUnordored`
- Better state apply and remove with `FutureUnordored` by [@cszach](https://github.com/cszach)
- Resource `Custom` renamed to `Kind` and allow custom url for hook by [@CreepyPvP](https://github.com/CreepyPvP)

### Fixed

- Unix socket permission when rebooting

## [0.7.0] - 2023-05-22

### Changed

- Vm runtime image to `ghcr.io/next-hat/nanocl-qemu:7.1.0.0`
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
- Upgrade ncproxy to 0.3
- Upgrade nproxy to 1.23.4.0
