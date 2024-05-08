# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.0] - 2024-05-08

### Changed

- Use the new event system from the daemon

### Added

- AllowedIps to allow specific ip address to reach exposed services

### Fixed

- Tcp with ssl
- Set a specific path for a target when using http

## [0.10.1] - 2023-12-30

### Fixed

- SSL redirection even when not enabled

## [0.10.0] - 2023-12-28

### Changed

- state-dir option to match with nproxy.
- Use of nanocld_client 0.13.0.
- Use of template system for config file generation.

### Added

- Metrics saving for http and stream.
- Reload task that can be canceled to avoid burst from large Statefile.
- Test file generation on creation and heal config if test fail.

### Fixed

- Openssl linking.

## [0.9.0] - 2023-12-22

### Changed

- Update to nanocld_client 0.12
- Added more logs

## [0.8.0] - 2023-11-06

### Changed

- Use of symbolic link to enable or not a config with `available` and `enabled` directories

## [0.7.0] - 2023-10-04

### Added

- Option All to bind all network interfaces [@anonkey](https://github.com/anonkey)

### Changed

- Public no longer bind all network interfaces [@anonkey](https://github.com/anonkey)
- Removed reopen on reload [@anonkey](https://github.com/anonkey)
- Better error handling on reload [@anonkey](https://github.com/anonkey)

## [0.6.0] - 2023-07-04

### Added

- Allow Stream and Http inside the same array

## [0.5.0] - 2023-06-03

### Changed

- Target use generic Target `Key`, `Port` and `Watch` require to specify if it's cargo or vm with `.c` or `.v`.
- The `ProxyRule` is now created at runtime.

### Added

- Options `DisableLogging` and `Path` for `CargoTarget` to disable logging for specific path.

### Fixed

- Thread background crash if `/var/log/nginx/access` doesn't exists.
- Updating a cargo wasn't refreshing the nginx config when using `nanocl cargo revert`.

## [0.4.3] - 2023-05-14

### Added

- SSL certificate as authentication

## [0.4.2] - 2023-05-14

### Fixed

- Config generation that make nginx crash when cargo instances have invalid ip address

## [0.4.1] - 2023-05-10

### Added

- `Headers` and `Version` options

## [0.4.0] - 2023-05-05

### Fixed

- network namespace binding

### Added

- http and tcp/udp json output norm for metrics
- Untagged rules and target for cleaner rule definition

### Fixed

- Image name

## [0.3.1] - 2023-04-14

### Fixed

- Reload command with correct cargo name

## [0.3.0] - 2023-04-15

### Added

- Event Watching from nanocl daemon
- Proxy Rule accept namespace as network
