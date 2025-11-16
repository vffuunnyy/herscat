use super::{
    SharedCounters, SocketTarget, StressConfig, build_payload, packet_interval, supervise_workers,
};
use anyhow::{Result, anyhow};
use rand::{Rng, rng};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_socks::tcp::Socks5Stream;

pub async fn run(
    config: &StressConfig,
    counters: SharedCounters,
    start_time: Instant,
) -> Result<()> {
    let targets = config.socket_targets();
    if targets.is_empty() {
        return Err(anyhow!(
            "No host:port targets configured for TCP flood mode"
        ));
    }
    let targets = Arc::new(targets);

    let payload = Arc::new(build_payload(config.packet_size));
    let packet_interval = packet_interval(config.packet_rate);
    let end_time = config.duration.map(|d| start_time + d);

    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    for (idx, port) in config.proxy_ports.iter().enumerate() {
        for worker in 0..config.concurrency {
            let params = TcpWorkerParams {
                worker_id: idx * 10_000 + worker,
                proxy_port: *port,
                targets: Arc::clone(&targets),
                payload: Arc::clone(&payload),
                packet_interval,
                end_time,
                packets_per_connection: config.packets_per_connection,
                counters: counters.clone(),
            };
            let handle = tokio::spawn(async move {
                tcp_worker_loop(params).await;
            });
            handles.push(handle);
        }
    }

    supervise_workers(handles, end_time).await
}

struct TcpWorkerParams {
    worker_id: usize,
    proxy_port: u16,
    targets: Arc<Vec<SocketTarget>>,
    payload: Arc<Vec<u8>>,
    packet_interval: Option<Duration>,
    end_time: Option<Instant>,
    packets_per_connection: Option<u32>,
    counters: SharedCounters,
}

async fn tcp_worker_loop(params: TcpWorkerParams) {
    loop {
        if let Some(end) = params.end_time
            && Instant::now() >= end
        {
            log::debug!(
                "TCP worker {} finished due to duration limit",
                params.worker_id
            );
            break;
        }

        let idx = rng().random_range(0..params.targets.len());
        let target = &params.targets[idx];

        match Socks5Stream::connect(
            ("127.0.0.1", params.proxy_port),
            (target.host.as_str(), target.port),
        )
        .await
        {
            Ok(mut stream) => {
                if let Err(err) = send_loop(&mut stream, &params).await {
                    log::debug!(
                        "TCP worker {} stream error towards {}: {}",
                        params.worker_id,
                        target.display(),
                        err
                    );
                    params.counters.record_failure();
                }
            }
            Err(err) => {
                log::debug!(
                    "TCP worker {} failed to connect via proxy {} -> {}: {}",
                    params.worker_id,
                    params.proxy_port,
                    target.display(),
                    err
                );
                params.counters.record_failure();
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn send_loop(stream: &mut Socks5Stream<TcpStream>, params: &TcpWorkerParams) -> Result<()> {
    let mut packets_this_connection = 0u32;

    loop {
        stream.write_all(&params.payload).await?;
        params.counters.record_packet(params.payload.len());
        packets_this_connection = packets_this_connection.saturating_add(1);

        if let Some(interval) = params.packet_interval {
            sleep(interval).await;
        }

        if let Some(limit) = params.packets_per_connection {
            if packets_this_connection >= limit {
                break;
            }
        }

        if let Some(end) = params.end_time
            && Instant::now() >= end
        {
            break;
        }
    }

    Ok(())
}
