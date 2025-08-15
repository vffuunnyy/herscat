use anyhow::{Context, Result};
use futures::StreamExt;
use futures::future::join_all;
use reqwest::{Client, Proxy};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const DEFAULT_TARGETS: &[&str] = &[
    "http://speedtest.tele2.net/10GB.zip",
    "http://ipv4.download.thinkbroadband.com/10GB.zip",
    "http://speedtest.tele2.net/1GB.zip",
    "http://speedtest.tele2.net/5GB.zip",
    "http://speedtest.tele2.net/100MB.zip",
    "http://ipv4.download.thinkbroadband.com/1GB.zip",
    "http://ipv4.download.thinkbroadband.com/5GB.zip",
    "http://ipv4.download.thinkbroadband.com/100MB.zip",
    "http://ipv4.download.thinkbroadband.com/50MB.zip",
    "http://ipv6.download.thinkbroadband.com/10GB.zip",
    "http://speedtest.bouyguestelecom.fr/1000Mo.zip",
    "http://proof.ovh.net/files/10Gb.dat",
    "http://proof.ovh.net/files/1Gb.dat",
    "http://proof.ovh.net/files/100Mb.dat",
    "http://speed.hetzner.de/10GB.bin",
    "http://speed.hetzner.de/1GB.bin",
    "http://speed.hetzner.de/100MB.bin",
    "http://mirror.leaseweb.com/speedtest/10000mb.bin",
    "http://mirror.leaseweb.com/speedtest/1000mb.bin",
    "http://mirror.leaseweb.com/speedtest/100mb.bin",
    "http://speedtest-sgp1.digitalocean.com/10gb.test",
    "http://speedtest-nyc1.digitalocean.com/10gb.test",
    "http://speedtest-fra1.digitalocean.com/10gb.test",
    "http://speedtest.newark.linode.com/100MB-newark.bin",
    "http://speedtest.atlanta.linode.com/100MB-atlanta.bin",
    "http://speedtest.london.linode.com/100MB-london.bin",
    "http://fra-de-ping.vultr.com/vultr.com.1000MB.bin",
    "http://lon-gb-ping.vultr.com/vultr.com.1000MB.bin",
    "http://par-fr-ping.vultr.com/vultr.com.1000MB.bin",
    "http://speedtest.scaleway.com/10G.iso",
    "http://speedtest.scaleway.com/1G.iso",
    "http://mirror.internode.on.net/pub/test/10meg.test",
    "http://mirror.internode.on.net/pub/test/100meg.test",
    "http://speed.cloudflare.com/10mb",
    "http://speed.cloudflare.com/100mb",
];

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "curl/7.88.1",
    "Wget/1.21",
];

#[inline]
fn jitter_range(min_ms: u64, max_ms: u64) -> Duration {
    let span = max_ms.saturating_sub(min_ms);
    let rnd = if span > 0 {
        (rand::random::<u8>() as u64) % span
    } else {
        0
    };
    Duration::from_millis(min_ms + rnd)
}

#[inline]
fn default_jitter() -> Duration {
    jitter_range(5, 15)
}

#[derive(Debug, Clone)]
pub struct StressConfig {
    pub targets: Vec<String>,
    pub concurrency: usize,
    pub duration: Option<Duration>,
    pub proxy_ports: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct StressStats {
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub bytes_downloaded: u64,
    pub start_time: Instant,
}

impl StressStats {
    pub fn new() -> Self {
        Self {
            successful_requests: 0,
            failed_requests: 0,
            bytes_downloaded: 0,
            start_time: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn bytes_per_second(&self) -> f64 {
        let elapsed_secs = self.elapsed().as_secs_f64();
        if elapsed_secs > 0.0 {
            self.bytes_downloaded as f64 / elapsed_secs
        } else {
            0.0
        }
    }
}

#[derive(Clone)]
struct WorkerParams {
    thread_id: usize,
    client: Client,
    targets: Vec<String>,
    concurrency: usize,
    end_time: Option<Instant>,
    successful_requests: Arc<AtomicU64>,
    failed_requests: Arc<AtomicU64>,
    bytes_downloaded: Arc<AtomicU64>,
}

pub struct StressRunner {
    config: StressConfig,
    clients: Vec<Client>,
    stats: StressStats,
    pub successful_requests: Arc<AtomicU64>,
    pub failed_requests: Arc<AtomicU64>,
    pub bytes_downloaded: Arc<AtomicU64>,
}

impl StressRunner {
    pub fn new(config: StressConfig) -> Result<Self> {
        let stats = StressStats::new();
        let successful_requests = Arc::new(AtomicU64::new(0));
        let failed_requests = Arc::new(AtomicU64::new(0));
        let bytes_downloaded = Arc::new(AtomicU64::new(0));

        let mut clients = Vec::new();
        for &port in &config.proxy_ports {
            let proxy = Proxy::all(format!("socks5://127.0.0.1:{port}"))
                .context("Failed to create proxy")?;

            let client = Client::builder()
                .proxy(proxy)
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(600))
                .danger_accept_invalid_certs(true)
                .tcp_keepalive(Duration::from_secs(60))
                .pool_idle_timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(10)
                .build()
                .context("Failed to create HTTP client")?;

            clients.push(client);
        }

        if clients.is_empty() {
            return Err(anyhow::anyhow!("No proxy clients available"));
        }

        Ok(Self {
            config,
            stats,
            successful_requests,
            failed_requests,
            bytes_downloaded,
            clients,
        })
    }

    pub async fn run(&self) -> Result<()> {
        log::info!(
            "Starting stress test with total concurrency = {} across {} xray clients",
            self.config.concurrency,
            self.clients.len()
        );

        let mut handles = Vec::new();
        let end_time = self.config.duration.map(|d| self.stats.start_time + d);

        // Distribute total concurrency across clients as evenly as possible
        let n = self.clients.len().max(1);
        let base = self.config.concurrency / n;
        let rem = self.config.concurrency % n;

        for (idx, client) in self.clients.iter().cloned().enumerate() {
            let client_concurrency = if idx < rem { base + 1 } else { base };
            if client_concurrency == 0 {
                continue;
            }
            let targets = self.config.targets.clone();
            let successful_requests = Arc::clone(&self.successful_requests);
            let failed_requests = Arc::clone(&self.failed_requests);
            let bytes_downloaded = Arc::clone(&self.bytes_downloaded);

            let params = WorkerParams {
                thread_id: idx,
                client,
                targets,
                concurrency: client_concurrency,
                end_time,
                successful_requests,
                failed_requests,
                bytes_downloaded,
            };

            let handle = tokio::spawn(async move { Self::worker_loop(params).await });
            handles.push(handle);
        }

        let results = join_all(handles).await;
        for (i, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                log::error!("Worker {i} panicked: {e}");
            }
        }

        log::debug!(
            "Final counters - Success: {}, Failed: {}, Bytes: {}",
            self.successful_requests.load(Ordering::Relaxed),
            self.failed_requests.load(Ordering::Relaxed),
            self.bytes_downloaded.load(Ordering::Relaxed)
        );

        Ok(())
    }

    async fn worker_loop(params: WorkerParams) {
        let thread_id = params.thread_id;

        loop {
            if let Some(end) = params.end_time && Instant::now() >= end {
                log::debug!("Worker {thread_id} stopping due to duration limit");
                break;
            }

            let mut download_handles = Vec::with_capacity(params.concurrency);

            for _ in 0..params.concurrency {
                let client = params.client.clone();
                let targets = params.targets.clone();
                let successful_requests = Arc::clone(&params.successful_requests);
                let failed_requests = Arc::clone(&params.failed_requests);
                let bytes_downloaded = Arc::clone(&params.bytes_downloaded);

                download_handles.push(tokio::spawn(Self::download_once(
                    client,
                    targets,
                    successful_requests,
                    failed_requests,
                    bytes_downloaded,
                )));
            }

            if let Some(end) = params.end_time {
                let time_left = end.saturating_duration_since(Instant::now());
                if time_left.as_millis() < 100 {
                    let _ = join_all(download_handles).await;
                    break;
                }
                let _ = tokio::time::timeout(time_left, join_all(download_handles)).await;
            } else {
                let _ = join_all(download_handles).await;
            }

            tokio::time::sleep(default_jitter()).await;
        }

        log::debug!("Worker {thread_id} completed");
    }

    async fn download_once(
        client: Client,
        targets: Vec<String>,
        successful_requests: Arc<AtomicU64>,
        failed_requests: Arc<AtomicU64>,
        bytes_downloaded: Arc<AtomicU64>,
    ) {
        let target_index = rand::random::<usize>() % targets.len();
        let ua_index = rand::random::<usize>() % USER_AGENTS.len();

        let target = &targets[target_index];
        let user_agent = USER_AGENTS[ua_index];

        match client
            .get(target)
            .header("User-Agent", user_agent)
            .send()
            .await
        {
            Ok(response) => {
                successful_requests.fetch_add(1, Ordering::Relaxed);

                let mut stream = response.bytes_stream();
                let mut total_bytes_this_request = 0u64;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let chunk_size = chunk.len() as u64;
                            total_bytes_this_request += chunk_size;
                            bytes_downloaded.fetch_add(chunk_size, Ordering::Relaxed);

                            if total_bytes_this_request % (10 * 1024 * 1024) == 0 {
                                log::debug!(
                                    "Downloaded {}MB from {}",
                                    total_bytes_this_request / (1024 * 1024),
                                    target
                                );
                            }
                        }
                        Err(e) => {
                            log::debug!(
                                "Stream error from {} after {}MB: {}",
                                target,
                                total_bytes_this_request / (1024 * 1024),
                                e
                            );
                            break;
                        }
                    }
                }

                if total_bytes_this_request > 0 {
                    log::debug!(
                        "Completed download from {}: {}MB total",
                        target,
                        total_bytes_this_request / (1024 * 1024)
                    );
                }
            }
            Err(e) => {
                log::debug!("Connection failed to {target}: {e}");
                failed_requests.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn get_current_stats(&self) -> StressStats {
        StressStats {
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            bytes_downloaded: self.bytes_downloaded.load(Ordering::Relaxed),
            start_time: self.stats.start_time,
        }
    }

    pub async fn start_stats_reporter(&self, interval: Duration) {
        let successful_requests = Arc::clone(&self.successful_requests);
        let failed_requests = Arc::clone(&self.failed_requests);
        let bytes_downloaded = Arc::clone(&self.bytes_downloaded);
        let start_time = self.stats.start_time;
        let end_time = self.config.duration.map(|d| start_time + d);

        tokio::spawn(async move {
            let mut last_successful = 0u64;
            let mut last_failed = 0u64;
            let mut last_bytes = 0u64;
            let mut ema_bytes_per_sec: Option<f64> = None;
            let alpha = 0.3;

            loop {
                sleep(interval).await;

                let current_successful = successful_requests.load(Ordering::Relaxed);
                let current_failed = failed_requests.load(Ordering::Relaxed);
                let current_bytes = bytes_downloaded.load(Ordering::Relaxed);

                let _successful_delta = current_successful - last_successful;
                let _failed_delta = current_failed - last_failed;
                let bytes_delta = current_bytes - last_bytes;

                let _elapsed = start_time.elapsed().as_secs_f64();

                let bytes_per_sec_instant = bytes_delta as f64 / interval.as_secs_f64();
                let ema_bps = match ema_bytes_per_sec {
                    Some(prev) => alpha * bytes_per_sec_instant + (1.0 - alpha) * prev,
                    None => bytes_per_sec_instant,
                };
                ema_bytes_per_sec = Some(ema_bps);
                let mb_per_sec = ema_bps / (1024.0 * 1024.0);
                let mbit_per_sec = (ema_bps * 8.0) / (1000.0 * 1000.0);
                let total_gb = current_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

                log::info!(
                    "[TRAFFIC] Speed(EMA): {:.2} MB/s ({:.0} Mbps) | Delta: {:.1} MB | Total: {:.2} GB",
                    mb_per_sec,
                    mbit_per_sec,
                    bytes_delta as f64 / (1024.0 * 1024.0),
                    total_gb
                );

                last_successful = current_successful;
                last_failed = current_failed;
                last_bytes = current_bytes;

                if let Some(end) = end_time && Instant::now() >= end {
                    break;
                }
            }
        });
    }
}

pub fn parse_custom_targets(targets_str: &str) -> Vec<String> {
    targets_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn get_default_targets() -> Vec<String> {
    DEFAULT_TARGETS.iter().map(|&s| s.to_string()).collect()
}
