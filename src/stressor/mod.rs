mod download;
mod tcp;
mod udp;

use crate::cli::Mode;
use crate::stressor::download::DEFAULT_HTTP_TARGETS;
use anyhow::{Result, anyhow};
use futures::future::join_all;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use url::Url;

#[derive(Debug, Clone)]
pub enum Target {
    Http(String),
    Socket(SocketTarget),
}

#[derive(Debug, Clone)]
pub struct SocketTarget {
    pub host: String,
    pub port: u16,
}

impl SocketTarget {
    pub fn display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct StressConfig {
    pub mode: Mode,
    pub targets: Vec<Target>,
    pub concurrency: usize,
    pub duration: Option<Duration>,
    pub proxy_ports: Vec<u16>,
    pub packet_size: usize,
    pub packet_rate: Option<u32>,
    pub packets_per_connection: Option<u32>,
}

impl StressConfig {
    pub fn http_targets(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter_map(|t| match t {
                Target::Http(url) => Some(url.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn socket_targets(&self) -> Vec<SocketTarget> {
        self.targets
            .iter()
            .filter_map(|t| match t {
                Target::Socket(target) => Some(target.clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct StressStats {
    pub success_events: u64,
    pub failure_events: u64,
    pub bytes_transferred: u64,
    pub packets_sent: u64,
    pub start_time: Instant,
}

impl StressStats {
    pub fn new() -> Self {
        Self {
            success_events: 0,
            failure_events: 0,
            bytes_transferred: 0,
            packets_sent: 0,
            start_time: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn bytes_per_second(&self) -> f64 {
        let elapsed_secs = self.elapsed().as_secs_f64();
        if elapsed_secs.is_normal() {
            self.bytes_transferred as f64 / elapsed_secs
        } else {
            0.0
        }
    }

    pub fn packets_per_second(&self) -> f64 {
        let elapsed_secs = self.elapsed().as_secs_f64();
        if elapsed_secs.is_normal() && elapsed_secs > 0.0 {
            self.packets_sent as f64 / elapsed_secs
        } else {
            0.0
        }
    }
}

#[derive(Clone)]
pub struct SharedCounters {
    pub success_events: Arc<AtomicU64>,
    pub failure_events: Arc<AtomicU64>,
    pub bytes_transferred: Arc<AtomicU64>,
    pub packets_sent: Arc<AtomicU64>,
}

impl SharedCounters {
    pub fn new() -> Self {
        Self {
            success_events: Arc::new(AtomicU64::new(0)),
            failure_events: Arc::new(AtomicU64::new(0)),
            bytes_transferred: Arc::new(AtomicU64::new(0)),
            packets_sent: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn record_success(&self) {
        self.success_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.failure_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_bytes(&self, bytes: u64) {
        self.bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_packet(&self, payload_bytes: usize) {
        self.record_success();
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_transferred
            .fetch_add(payload_bytes as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self, start_time: Instant) -> StressStats {
        StressStats {
            success_events: self.success_events.load(Ordering::Relaxed),
            failure_events: self.failure_events.load(Ordering::Relaxed),
            bytes_transferred: self.bytes_transferred.load(Ordering::Relaxed),
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            start_time,
        }
    }
}

#[derive(Clone)]
pub struct StressRunner {
    config: StressConfig,
    counters: SharedCounters,
    stats: StressStats,
}

impl StressRunner {
    pub fn new(config: StressConfig) -> Result<Self> {
        if config.proxy_ports.is_empty() {
            return Err(anyhow!("No proxy ports provided for stress runner"));
        }

        Ok(Self {
            config,
            counters: SharedCounters::new(),
            stats: StressStats::new(),
        })
    }

    pub async fn run(&self) -> Result<()> {
        match self.config.mode {
            Mode::Download => {
                download::run(&self.config, self.counters.clone(), self.stats.start_time).await
            }
            Mode::TcpFlood => {
                tcp::run(&self.config, self.counters.clone(), self.stats.start_time).await
            }
            Mode::UdpFlood => {
                udp::run(&self.config, self.counters.clone(), self.stats.start_time).await
            }
        }
    }

    pub async fn start_stats_reporter(&self, interval: Duration) {
        let counters = self.counters.clone();
        let mode = self.config.mode;
        let start_time = self.stats.start_time;
        let end_time = self.config.duration.map(|d| start_time + d);

        tokio::spawn(async move {
            let mut last_bytes = 0u64;
            let mut last_packets = 0u64;
            loop {
                sleep(interval).await;

                let bytes = counters.bytes_transferred.load(Ordering::Relaxed);
                let packets = counters.packets_sent.load(Ordering::Relaxed);
                let bytes_delta = bytes - last_bytes;
                let packets_delta = packets - last_packets;

                let seconds = interval.as_secs_f64().max(1.0);
                let mb_per_sec = (bytes_delta as f64 / seconds) / (1024.0 * 1024.0);
                let mbit_per_sec = (bytes_delta as f64 * 8.0) / (seconds * 1_000_000.0);
                let pps = packets_delta as f64 / seconds;
                let total_gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);

                match mode {
                    Mode::Download => {
                        log::info!(
                            "[HTTP] Speed: {:.2} MB/s ({:.0} Mbps) | Delta: {:.1} MB | Total: {:.2} GB",
                            mb_per_sec,
                            mbit_per_sec,
                            bytes_delta as f64 / (1024.0 * 1024.0),
                            total_gb
                        );
                    }
                    Mode::TcpFlood => {
                        log::info!(
                            "[TCP] PPS: {:.0} | Throughput: {:.2} MB/s ({:.0} Mbps) | Total: {:.2} GB",
                            pps,
                            mb_per_sec,
                            mbit_per_sec,
                            total_gb
                        );
                    }
                    Mode::UdpFlood => {
                        log::info!(
                            "[UDP] PPS: {:.0} | Throughput: {:.2} MB/s ({:.0} Mbps) | Total: {:.2} GB",
                            pps,
                            mb_per_sec,
                            mbit_per_sec,
                            total_gb
                        );
                    }
                }

                last_bytes = bytes;
                last_packets = packets;

                if let Some(end) = end_time
                    && Instant::now() >= end
                {
                    break;
                }
            }
        });
    }

    pub fn get_current_stats(&self) -> StressStats {
        self.counters.snapshot(self.stats.start_time)
    }

    pub fn mode(&self) -> Mode {
        self.config.mode
    }
}

pub fn resolve_targets(mode: Mode, raw: Option<&str>) -> Result<Vec<Target>> {
    if let Some(spec) = raw {
        return parse_target_list(spec, mode);
    }

    match mode {
        Mode::Download => Ok(DEFAULT_HTTP_TARGETS
            .iter()
            .map(|url| Target::Http((*url).to_string()))
            .collect()),
        Mode::TcpFlood | Mode::UdpFlood => Err(anyhow!(
            "Mode {mode:?} requires --targets with host:port entries"
        )),
    }
}

pub fn parse_target_list(raw: &str, mode: Mode) -> Result<Vec<Target>> {
    let mut targets = Vec::new();
    for chunk in raw.split(',') {
        let token = chunk.trim();
        if token.is_empty() {
            continue;
        }

        let target = match mode {
            Mode::Download => parse_http_target(token)?,
            Mode::TcpFlood | Mode::UdpFlood => parse_socket_target(token)?,
        };
        targets.push(target);
    }

    if targets.is_empty() {
        return Err(anyhow!("No targets parsed from input"));
    }

    Ok(targets)
}

fn parse_http_target(token: &str) -> Result<Target> {
    let url = Url::parse(token).map_err(|e| anyhow!("Invalid HTTP target {token}: {e}"))?;
    match url.scheme() {
        "http" | "https" => Ok(Target::Http(token.to_string())),
        _ => Err(anyhow!(
            "Unsupported scheme for HTTP target: {}",
            url.scheme()
        )),
    }
}

fn parse_socket_target(token: &str) -> Result<Target> {
    let (host, port_str) = if token.starts_with('[') {
        let closing = token
            .find(']')
            .ok_or_else(|| anyhow!("Invalid IPv6 host syntax in {token}"))?;
        let rest = &token[closing + 1..];
        if !rest.starts_with(':') {
            return Err(anyhow!(
                "Expected port delimiter ':' for IPv6 host in {token}"
            ));
        }
        (&token[1..closing], &rest[1..])
    } else if let Some((host, port)) = token.rsplit_once(':') {
        (host, port)
    } else {
        return Err(anyhow!(
            "Socket target must be in host:port format (got {token})"
        ));
    };

    if host.is_empty() {
        return Err(anyhow!("Socket target missing host in {token}"));
    }

    let port: u16 = port_str
        .parse()
        .map_err(|_| anyhow!("Invalid port in socket target {token}"))?;

    Ok(Target::Socket(SocketTarget {
        host: host.to_string(),
        port,
    }))
}

pub(crate) fn build_payload(size: usize) -> Vec<u8> {
    use rand::Rng;
    let mut payload = vec![0u8; size.max(1)];
    rand::rng().fill(payload.as_mut_slice());
    payload
}

pub(crate) fn packet_interval(rate: Option<u32>) -> Option<Duration> {
    rate.and_then(|pps| {
        if pps == 0 {
            None
        } else {
            Some(Duration::from_secs_f64(1.0 / pps as f64))
        }
    })
}

pub(crate) async fn supervise_workers(
    handles: Vec<JoinHandle<()>>,
    end_time: Option<Instant>,
) -> Result<()> {
    if handles.is_empty() {
        return Err(anyhow!("No worker tasks spawned"));
    }

    if let Some(end) = end_time {
        let now = Instant::now();
        if end > now {
            sleep(end - now).await;
        }
        for handle in &handles {
            handle.abort();
        }
    }

    let results = join_all(handles).await;
    for (idx, result) in results.into_iter().enumerate() {
        if let Err(e) = result {
            log::error!("Worker {idx} panicked: {e}");
        }
    }

    Ok(())
}
