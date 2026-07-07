# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Upstream tracking

This crate is a Rust port of the Node.js package [`@botiverse/raft-daemon`](https://www.npmjs.com/package/@botiverse/raft-daemon).
To keep the two in sync, each release records the upstream npm version it tracks.

| raft-daemon (Rust) | Upstream `@botiverse/raft-daemon` |
|--------------------|-----------------------------------|
| 0.69.0             | 0.69.0                            |

## [Unreleased]

### Changed

- Initial Rust port from `@botiverse/raft-daemon` 0.69.0.

### Tooling

- Cross-compilation build scripts (macOS / Linux gnu+musl / FreeBSD).
  Codesigning and notarization are env-driven (`CODESIGN_IDENTITY`, `NOTARY_PROFILE`),
  disabled by default. Removed hardcoded signing identity and Android build step.
