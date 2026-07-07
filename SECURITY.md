# Security Policy

## Reporting a Vulnerability

We take security bugs seriously and appreciate your efforts to disclose them
responsibly.

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, report them privately using one of these channels:

- Open a private security advisory via GitHub:
  **Report a vulnerability** on the
  [Security advisories](../../security/advisories/new) tab.
- Email the maintainers at **security@raft.build** with a description of the
  issue, steps to reproduce, and any proof-of-concept.

We aim to acknowledge reports within **72 hours** and to provide an initial
assessment within **7 days**. Coordinated disclosure timelines are decided
case by case with the reporter.

## Supported Versions

Only the latest released version receives security fixes. Older releases are
supported on a best-effort basis.

| Version | Supported          |
|---------|--------------------|
| 0.69.x | :white_check_mark: |
| < 0.69 | :x:                |

## Scope

The following are considered in scope:

- The `raft-daemon` Rust crate and its CLI.
- The bundled runtime drivers (`rusty`, `builtin`).
- Handling of credentials (API keys passed via `RAFT_API_KEY`) and on-disk
  state files.

The following are out of scope:

- The hosted Raft service at `raft.build` and its web/API surface.
- Third-party runtime binaries (for example RustyCLI) that the daemon invokes.

## Hardening Notes for Operators

- The daemon writes its state file with `0600` permissions because it may
  contain API keys. Do not weaken these permissions.
- API keys are passed to spawned runtimes via the `RAFT_API_KEY` environment
  variable, never as a command-line argument, to avoid leaking them through
  process listings (`ps`, `/proc`).
- The server connection requires a `wss://` URL; plaintext `ws://` is refused
  by default.
