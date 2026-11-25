# Changelog

All notable changes to Squad will be documented in this file, following
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

No release has been published. The workspace version is 0.1.0. This record
summarises implemented work in git history; the PRD also describes capabilities
that are still under construction.

### Added

- Six-crate Rust workspace: protocol, store, agents, core, TUI, and CLI.
- CLI command surface, private storage paths, quiet logging, configuration
  loading, environment precedence, limits, and doctor checks.
- Setup script with pinned-toolchain installation, rerun handling, and CI gates.
- Command parser for addressing, modes, variables, picks, and feedback.
- OpenRouter streaming SSE transport, retry and cancellation handling, and a
  fake provider for offline tests.
- Incremental streamed Markdown rendering with highlighted code.
- Contest boundary banners and 46 prompt templates.
- Release scaffolding for macOS arm64 and Linux x64 archives and SHA-256
  checksums, a Homebrew formula template, and CLI package metadata. Publication
  and full supported-platform installation evidence remain pending.

### Fixed

- Startup credential exposure through non-UTF-8 environment errors.
- Doctor configuration reporting and setup script toolchain handling.

### Planned after v1

- `$lattice`, `$delphi`, `$assemble`, and a spending-controlled meta-mode that
  loops `$grill` and `$census` until quiet.
- Additional architecture targets, signing/notarisation, multi-room operation,
  remote attachment, tools, and image/file uploads, as tracked in the launch PRD.
