# Changelog


All notable changes to this project will be documented in this file.

The format follows Keep a Changelog and the project adheres to Semantic Versioning.

## [0.2.0-pre] - 2025-11-16

### Added
- New `--mode` selector with `download`, `tcp-flood`, and `udp-flood` executors that all tunnel
  traffic through the launched SOCKS5 proxies.
- Per-mode target parsing via shared `--targets` flag (HTTP URLs for downloads, `host:port` for floods).
- Flood-specific knobs: `--packet-size` and `--packet-rate` for shaping TCP/UDP payload streams.
- `--packets-per-conn` option to control how many packets are sent before TCP/UDP flood connections
  are re-established (set to 1 for per-packet churn).
- Refactored stressor into dedicated modules with improved live stats (MB/s, Mbps, PPS, total packets).
- Added handy short aliases for common flags (`-t/--targets`, `-m/--mode`, etc.) for quicker CLI use.
- Updated VLESS parser/generator to understand modern Xray fields (ML-KEM `encryption`, `packetEncoding`,
  REALITY `spiderX`, reverse tags, xor/seconds/padding hints) so new share links work out of the box.

### Changed
- Final statistics and periodic reports now adapt to the active stress mode.
- README and CLI reference updated with the new workflows and examples.

## [0.1.1] - 2025-08-15

### Changed
- Docker image now bundles `xray-core`; external mount no longer required.
- Updated Docker usage examples in README.

### Fixed
- README typos and outdated notes.

## [0.1.0] - 2025-08-15

### Added
- Initial public release of HersCat.
- VLESS/Trojan/SS URL parsing and validation (TCP, WebSocket, gRPC; TLS/Reality).
- Xray-core configuration generation and multi-instance process management.
- SOCKS5-based distribution of HTTP download streams across instances.
- High-intensity HTTP stress testing with a single global concurrency parameter.
- Real-time statistics reporting with colored CLI output.
- Graceful shutdown handling (Ctrl+C).
- Custom target URLs for downloads.
- Shell completions generation (bash, zsh, fish).
- Comprehensive CLI with validation and helpful error messages.

### CI/Release
- GoReleaser pipeline set up:
  - Cross-platform builds via cargo-zigbuild.
  - Archive and checksum generation.
  - Signing (cosign/GPG) and cargo publish hook.
  - GitHub Actions workflows for tagged releases.

[0.1.0]: https://github.com/vffuunnyy/herscat/releases/tag/v0.1.0
