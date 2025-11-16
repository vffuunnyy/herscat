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
- Run thousands of concurrent HTTP downloads or TCP/UDP flood streams via SOCKS5
- Generate xray-core configs from proxy links (VLESS/Trojan/SS)
- Single URL (`--url`) or a list file (`--list`)
- Real-time statistics and colored output
- Understands modern VLESS options
- Configurable packets-per-connection limits for TCP/UDP floods to churn through proxy sessions faster
- Graceful shutdown (Ctrl+C) with clean process teardown
- Shell completions generator (bash, zsh, fish)
- High-performance async runtime (Tokio)

## Why HersCat?

HersCat orchestrates multiple xray-core instances and distributes HTTP download, TCP flood, and UDP flood traffic across them using SOCKS5 to help you evaluate performance stability, concurrency limits, and resilience of your proxy setup.

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
  ghcr.io/vffuunnyy/herscat:latest \
  --url "vless://uuid@server.com:443?type=tcp&security=tls&sni=server.com"

# Run with a list file mounted into the container
docker run --rm -it \
  --network host \
  -v $(pwd)/proxies.txt:/data/proxies.txt:ro \
  ghcr.io/vffuunnyy/herscat:latest \
  --list /data/proxies.txt --concurrency 2000 --instances 10 --duration 300
```

Notes:
- Since v0.1.1 the Docker image bundles `xray-core`; no extra mount is required.
- `--network host` is recommended so the containerized tool can open SOCKS5 ports and generate load using host networking.

## Quick Start

```bash
# Single proxy URL (supports vless/trojan/ss)
herscat --url "vless://uuid@server.com:443?type=tcp&security=tls&sni=server.com"

# Multiple proxy URLs from a file (one per line)
herscat --list proxies.txt --mode download --targets "http://example.com/1gb.zip"

# TCP flood against host:port targets via proxies
herscat \
  --mode tcp-flood \
  --list proxies.txt \
  --targets "203.0.113.10:443,example.org:80" \
  --packet-size 512 \
  --packet-rate 200 \
  --packets-per-conn 1

# UDP flood (same targets syntax as TCP)
herscat \
  --mode udp-flood \
  --list proxies.txt \
  --targets "198.51.100.5:53" \
  --packet-size 128 \
  --packets-per-conn 5
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
  -c, --concurrency <N>           Total concurrency per mode across all instances [default: 200]
  -t, --targets <ITEMS>           Mode-dependent targets (HTTP URLs or host:port entries)
  -m, --mode <MODE>               Stress mode: download|tcp-flood|udp-flood [default: download]
  -s, --packet-size <BYTES>       Packet size for tcp/udp flood payloads [default: 1024]
  -r, --packet-rate <PPS>         Optional per-task packets-per-second cap for tcp/udp flood
  -P, --packets-per-conn <COUNT>  Packets per TCP/UDP connection before reconnect (0 = keep open)
  -v, --verbose                   Info logging
      --debug                     Debug logging
  -i, --stats-interval <SECONDS>  Statistics reporting interval [default: 5]
  -h, --help                      Print help
  -V, --version                   Print version

Commands:
  completions <shell>             Generate shell completions (bash|zsh|fish)
```

`--targets` is shared across modes: supply HTTP/HTTPS URLs for `download`, and `host:port` pairs
for `tcp-flood` or `udp-flood`. Flood modes require explicit targets, while the download mode falls
back to the built-in list if none is provided.

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

### Custom HTTP download and flood targets

```bash
# Download mode with custom URLs
herscat \
  --mode download \
  --url "vless://uuid@server.com:443?type=tcp&security=tls" \
  --targets "http://example.com/1gb.zip,http://example.net/5gb.zip" \
  --concurrency 200

# TCP/UDP flood modes
herscat \
  --mode tcp-flood \
  --list proxies.txt \
  --targets "target1:443,target2:80" \
  --packet-size 256 --packets-per-conn 2

herscat \
  --mode udp-flood \
  --list proxies.txt \
  --targets "target3:53" \
  --packet-size 128 --packet-rate 500
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
