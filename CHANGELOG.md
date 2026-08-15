# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/JayanAXHF/portside/compare/v0.4.0...v0.5.0) - 2026-08-15

### Added

- add command history ([#10](https://github.com/JayanAXHF/portside/pull/10))
- add themes support to portside ([#9](https://github.com/JayanAXHF/portside/pull/9))
- add discord RPC support

### Other

- *(README)* add cargo-binstall download instructions

## [0.4.0](https://github.com/JayanAXHF/portside/compare/v0.3.2...v0.4.0) - 2026-08-11

### Added

- add a now-playing indicator on top that shows currently playing music

### Fixed

- fix session list overflowing again

### Other

- fix inconsistencies in CI binary building
- yet another bugfix in build-binaries.yml

## [0.3.2](https://github.com/JayanAXHF/portside/compare/v0.3.1...v0.3.2) - 2026-08-09

### Other

- fix inconsistencies between cargo-binstall metadata and ci

## [0.3.1](https://github.com/JayanAXHF/portside/compare/v0.3.0...v0.3.1) - 2026-08-09

### Other

- add VHS demo tape to the README
- fix cargo-binstall metadata

## [0.3.0](https://github.com/JayanAXHF/portside/compare/v0.2.0...v0.3.0) - 2026-08-09

### Added

- add a history pane with graphs for daily, weekly and cumulative history
- add a clock that shows the current local time

### Fixed

- fixed sessions list overflowing and clipping
- fix today's total not counting resumed sessions from earlier days
- make commands with multiple `:`s still parse

### Other

- fix owner name in build-binaries.yml

## [0.2.0](https://github.com/JayanAXHF/portside/compare/v0.1.0...v0.2.0) - 2026-08-06

### Added

- add today's total and timed breaks with notifications
- make `<space>` toggle timer
- *(ci)* added cargo-binstall and cross-platform binaries

### Fixed

- change repo owner in release pls file
