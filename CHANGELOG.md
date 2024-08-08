# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

<!-- next-header -->

## [Unreleased] - ReleaseDate

### Added

 - Added the `x86_64-unknown-linux-musl` target to the `cargo-dist` configuration to support installing
   on systems where the glibc version is too old.

## [0.1.1] - 2024-08-08

### Added

 - Added `shell` and `powershell` installers to the `cargo-dist` configuration to provide easy
   installation on compatible systems.

## [0.1.0] - 2024-08-08

### Added

The `wall-a` CLI tool is intended to support writing data into a binary format
in the context of a git repository.

See the project [README](https://github.com/declanvk/wall-a/blob/main/README.md)
for a breakdown of the motivation and design.

<!-- next-url -->
[Unreleased]: https://github.com/declanvk/wall-a/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/declanvk/wall-a/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/declanvk/wall-a/compare/7c0ddf9fe8087f5dd530d9a3e5e3a1bd492cff34...v0.1.0
