use super::{SharedCounters, StressConfig, supervise_workers};
use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use rand::{Rng, rng};
use reqwest::{Client, Proxy};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

pub const DEFAULT_HTTP_TARGETS: &[&str] = &[
    "http://speedtest.tele2.net/1GB.zip",
    "http://speedtest.tele2.net/100MB.zip",
    "http://speedtest.tele2.net/10GB.zip",
    "http://ipv4.download.thinkbroadband.com/10GB.zip",
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
    "https://speed.cloudflare.com/__down?bytes=1000000",
    "https://speed.cloudflare.com/__down?bytes=10000000",
];

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "curl/7.88.1",
    "Wget/1.21",
];

pub async fn run(
    config: &StressConfig,
    counters: SharedCounters,
    start_time: Instant,
) -> Result<()> {
    let targets = config.http_targets();
    if targets.is_empty() {
        return Err(anyhow!("No HTTP targets configured for download mode"));
    }

    let mut clients = Vec::new();
    for &port in &config.proxy_ports {
        let proxy = Proxy::all(format!("socks5://127.0.0.1:{port}"))
            .context("Failed to configure SOCKS5 proxy")?;

        let client = Client::builder()
            .proxy(proxy)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(600))
            .danger_accept_invalid_certs(true)
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        clients.push(client);
    }

    if clients.is_empty() {
        return Err(anyhow!("No HTTP clients available"));
    }

    let targets = Arc::new(targets);
    let end_time = config.duration.map(|d| start_time + d);
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for (idx, client) in clients.into_iter().enumerate() {
        for worker in 0..config.concurrency {
            let worker_id = idx * 10_000 + worker;
            let client_clone = client.clone();
            let targets_clone = Arc::clone(&targets);
            let counters_clone = counters.clone();
            let handle = tokio::spawn(async move {
                match build_requests(&client_clone, &targets_clone) {
                    Ok(requests) => {
                        let params = WorkerParams {
                            thread_id: worker_id,
                            client: client_clone,
                            requests: Arc::new(requests),
                            end_time,
                            counters: counters_clone,
                        };
                        http_worker_loop(params).await;
                    }
                    Err(err) => {
                        log::error!("Failed to build requests: {err}");
                    }
                }
            });
            handles.push(handle);
        }
    }

    supervise_workers(handles, end_time).await
}

struct WorkerParams {
    thread_id: usize,
    client: Client,
    requests: Arc<Vec<reqwest::Request>>,
    end_time: Option<Instant>,
    counters: SharedCounters,
}

async fn http_worker_loop(params: WorkerParams) {
    let req_len = params.requests.len();
    let thread_id = params.thread_id;

    loop {
        if let Some(end) = params.end_time
            && Instant::now() >= end
        {
            log::debug!("HTTP worker {thread_id} stopping due to duration limit");
            break;
        }

        let idx = rng().random_range(0..req_len);
        let req = match params.requests[idx].try_clone() {
            Some(req) => req,
            None => {
                log::warn!("Failed to clone HTTP request (reqwest dropped body)");
                continue;
            }
        };

        execute_request(&params.client, req, &params.counters).await;
    }

    log::debug!("HTTP worker {thread_id} completed");
}

async fn execute_request(client: &Client, request: reqwest::Request, counters: &SharedCounters) {
    let target = request.url().to_string();
    match client.execute(request).await {
        Ok(response) => {
            counters.record_success();
            let mut stream = response.bytes_stream();
            let mut total_bytes = 0u64;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let chunk_size = chunk.len() as u64;
                        total_bytes += chunk_size;
                        counters.record_bytes(chunk_size);
                    }
                    Err(err) => {
                        log::debug!(
                            "Stream error from {} after {}MB: {}",
                            target,
                            total_bytes / (1024 * 1024),
                            err
                        );
                        counters.record_failure();
                        break;
                    }
                }
            }

            if total_bytes > 0 {
                log::debug!(
                    "Completed download from {}: {}MB total",
                    target,
                    total_bytes / (1024 * 1024)
                );
            }
        }
        Err(err) => {
            log::debug!("Connection failed to {target}: {err}");
            counters.record_failure();
        }
    }
}

fn build_requests(client: &Client, targets: &[String]) -> Result<Vec<reqwest::Request>> {
    let mut requests = Vec::with_capacity(targets.len());

    for target in targets {
        let user_agent = USER_AGENTS[rng().random_range(0..USER_AGENTS.len())];
        let req = client
            .get(target)
            .header("User-Agent", user_agent)
            .build()
            .with_context(|| format!("Failed to build request for {target}"))?;
        requests.push(req);
    }

    Ok(requests)
}
