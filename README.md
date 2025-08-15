# HersCat

![Crates.io Version](https://img.shields.io/crates/v/herscat) ![GitHub Tag](https://img.shields.io/github/v/tag/vffuunnyy/herscat) ![GitHub Repo stars](https://img.shields.io/github/stars/vffuunnyy/herscat)

![Build Status](https://img.shields.io/github/actions/workflow/status/vffuunnyy/herscat/release.yml) ![Crates.io Last Update](https://img.shields.io/crates/last-update/herscat)
![Downloads](https://img.shields.io/crates/d/herscat) ![License](https://img.shields.io/crates/l/herscat)

High-intensity xray proxy stress tester in Rust.

<p align="center">
  <img src="assets/logo.png" alt="HersCat Logo"/>
</p>

**⚠️ For controlled load testing of proxy deployments you own or have explicit permission to test.**

## Features

- Launch multiple xray-core instances automatically
- Run hundreds/thousands of concurrent HTTP download streams via SOCKS5
- Generate xray-core configs from proxy links (VLESS/Trojan/SS)
- Single URL (`--url`) or a list file (`--list`)
- Real-time statistics and colored output
- Graceful shutdown (Ctrl+C) with clean process teardown
- Shell completions generator (bash, zsh, fish)
- High-performance async runtime (Tokio)

## Why HersCat?

HersCat orchestrates multiple xray-core instances and distributes HTTP download traffic across them using SOCKS5 to help you evaluate performance stability, concurrency limits, and resilience of your proxy setup.

## Requirements

- Linux (tested on ArchLinux 22.04+)
- Rust toolchain (edition = 2024) — install via https://rustup.rs/
- `xray-core` available in PATH
- Common utilities: curl, wget, unzip, git

### Install xray-core quickly

```bash
wget https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-64.zip
unzip Xray-linux-64.zip
sudo mv xray /usr/local/bin/
sudo chmod +x /usr/local/bin/xray
```

## Installation

You can use any of the following:

- Prebuilt binaries: Download from GitHub Releases (archives include checksums; optional signatures if enabled by CI).
- Cargo:
```bash
  cargo install herscat
```
- From source:
```bash
  git clone https://github.com/vffuunnyy/herscat.git
  cd herscat
  cargo build --release
  # or install into ~/.cargo/bin
  cargo install --path .
  ```
- Docker: A minimal image built via GoReleaser can be used to run the tool in a container environment.
- Arch Linux (AUR) [PLANNED]:
  ```bash
  pacman -S herscat-bin
  yay -S herscat-bin
  paru -S herscat-bin

  # etc.
  ```

## Docker

Prebuilt images are published to GitHub Container Registry via GoReleaser.

```bash
# Pull the latest image
docker pull ghcr.io/vffuunnyy/herscat:latest

# Run with a single proxy URL
docker run --rm -it \
  --network host \
  -v /usr/local/bin/xray:/usr/local/bin/xray:ro \
  ghcr.io/vffuunnyy/herscat:latest \
  --url "vless://uuid@server.com:443?type=tcp&security=tls&sni=server.com"

# Run with a list file mounted into the container
docker run --rm -it \
  --network host \
  -v /usr/local/bin/xray:/usr/local/bin/xray:ro \
  -v $(pwd)/proxies.txt:/data/proxies.txt:ro \
  ghcr.io/vffuunnyy/herscat:latest \
  --list /data/proxies.txt --concurrency 2000 --instances 10 --duration 300
```

Notes:
- HersCat launches local xray-core instances; ensure the xray binary exists on the host at `/usr/local/bin/xray` and is mounted read-only into the container as shown above.
- `--network host` is recommended so the containerized tool can open SOCKS5 ports and generate load using host networking.

## Quick Start

```bash
# Single proxy URL (supports vless/trojan/ss)
herscat --url "vless://uuid@server.com:443?type=tcp&security=tls&sni=server.com"

# Multiple proxy URLs from a file (one per line)
herscat --list proxies.txt
```

## CLI Reference

```text
Usage: herscat [OPTIONS] [COMMAND]

Options:
  -u, --url <PROXY_URL>           Proxy URL (vless/trojan/ss)
  -l, --list <FILE>               File with proxy URLs, one per line
  -d, --duration <SECONDS>        Test duration in seconds (0 = infinite) [default: 0]
  -x, --instances <N>             Number of xray-core instances [default: 5]
  -p, --base-port <PORT>          Base SOCKS5 port [default: 10808]
  -c, --concurrency <N>           Total concurrent downloads across all instances [default: 200]
      --targets <URLS>            Comma-separated custom target URLs for downloads
  -v, --verbose                   Info logging
      --debug                     Debug logging
      --stats-interval <SECONDS>  Statistics reporting interval [default: 5]
  -h, --help                      Print help
  -V, --version                   Print version

Commands:
  completions <shell>             Generate shell completions (bash|zsh|fish)
```

## Examples

### High-intensity stress test

```bash
herscat \
  --list servers.txt \
  --concurrency 10000 \
  --instances 20 \
  --duration 600 \
  --verbose
```

### Custom download targets

```bash
herscat \
  --url "vless://uuid@server.com:443?type=tcp&security=tls" \
  --targets "http://example.com/1gb.zip,http://example.com/5gb.zip" \
  --concurrency 100
```

### Shell completions

```bash
# bash
herscat completions bash > ~/.local/share/bash-completion/completions/herscat
# zsh
herscat completions zsh > ~/.local/share/zsh/site-functions/_herscat
# fish
herscat completions fish > ~/.config/fish/completions/herscat.fish
```

## Safety and Ethics

⚠️ IMPORTANT DISCLAIMER

This tool is built for legitimate penetration testing, benchmarking, and research. You must:
- Test only systems you own or have explicit written permission to test
- Follow applicable laws and acceptable use policies
- Avoid any malicious or unauthorized activity

The authors and contributors are not responsible for misuse.

## Contributing

- Fork and create a feature branch
- Add tests and run checks:
  ```bash
  cargo fmt
  cargo clippy
  cargo test
  ```
- Submit a pull request with a clear description and rationale

## License

Licensed under either of:
- Apache License, Version 2.0 — see `LICENSE-APACHE`
- MIT License — see `LICENSE-MIT`

You may choose either license.

## Acknowledgments

- [Inspired by HellCat Go implementation](https://github.com/hellcat443/hellcat)
- [xray-core](https://github.com/XTLS/Xray-core)
