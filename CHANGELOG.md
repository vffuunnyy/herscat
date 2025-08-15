# Changelog


All notable changes to this project will be documented in this file.

The format follows Keep a Changelog and the project adheres to Semantic Versioning.

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
