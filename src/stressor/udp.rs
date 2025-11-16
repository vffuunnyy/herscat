use super::{
    SharedCounters, SocketTarget, StressConfig, build_payload, packet_interval, supervise_workers,
};
use anyhow::{Result, anyhow};
use rand::{Rng, rng};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub async fn run(
    config: &StressConfig,
    counters: SharedCounters,
    start_time: Instant,
) -> Result<()> {
    let targets = config.socket_targets();
    if targets.is_empty() {
        return Err(anyhow!(
            "No host:port targets configured for UDP flood mode"
        ));
    }
    let targets = Arc::new(targets);

    let payload = Arc::new(build_payload(config.packet_size));
    let packet_interval = packet_interval(config.packet_rate);
    let end_time = config.duration.map(|d| start_time + d);

    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    for (idx, port) in config.proxy_ports.iter().enumerate() {
        for worker in 0..config.concurrency {
            let params = UdpWorkerParams {
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
                udp_worker_loop(params).await;
            });
            handles.push(handle);
        }
    }

    supervise_workers(handles, end_time).await
}

struct UdpWorkerParams {
    worker_id: usize,
    proxy_port: u16,
    targets: Arc<Vec<SocketTarget>>,
    payload: Arc<Vec<u8>>,
    packet_interval: Option<Duration>,
    end_time: Option<Instant>,
    packets_per_connection: Option<u32>,
    counters: SharedCounters,
}

async fn udp_worker_loop(params: UdpWorkerParams) {
    let mut association: Option<UdpAssociation> = None;
    let mut packets_this_connection = 0u32;

    loop {
        if let Some(end) = params.end_time
            && Instant::now() >= end
        {
            log::debug!(
                "UDP worker {} finished due to duration limit",
                params.worker_id
            );
            break;
        }

        if association.is_none() {
            match UdpAssociation::connect(params.proxy_port).await {
                Ok(assoc) => association = Some(assoc),
                Err(err) => {
                    log::debug!(
                        "UDP worker {} failed to establish SOCKS association on port {}: {}",
                        params.worker_id,
                        params.proxy_port,
                        err
                    );
                    params.counters.record_failure();
                    sleep(Duration::from_millis(250)).await;
                    continue;
                }
            }
            packets_this_connection = 0;
        }

        let mut reset_association = false;
        if let Some(assoc) = association.as_mut() {
            match send_udp_packet(assoc, &params).await {
                Ok(()) => {
                    packets_this_connection = packets_this_connection.saturating_add(1);
                    if let Some(limit) = params.packets_per_connection
                        && packets_this_connection >= limit
                    {
                        reset_association = true;
                    }
                }
                Err(err) => {
                    log::debug!(
                        "UDP worker {} send error via proxy {}: {}",
                        params.worker_id,
                        params.proxy_port,
                        err
                    );
                    params.counters.record_failure();
                    reset_association = true;
                    sleep(Duration::from_millis(200)).await;
                }
            }
        }

        if reset_association {
            association = None;
            packets_this_connection = 0;
        }
    }
}

struct UdpAssociation {
    #[allow(dead_code)]
    tcp_guard: TcpStream,
    udp_socket: UdpSocket,
    relay_addr: SocketAddr,
}

impl UdpAssociation {
    async fn connect(proxy_port: u16) -> Result<Self> {
        let mut stream = TcpStream::connect(("127.0.0.1", proxy_port)).await?;
        perform_greeting(&mut stream).await?;
        let relay_addr = request_udp_associate(&mut stream).await?;
        let udp_socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).await?;

        Ok(Self {
            tcp_guard: stream,
            udp_socket,
            relay_addr,
        })
    }
}

async fn perform_greeting(stream: &mut TcpStream) -> Result<()> {
    let request = [0x05, 0x01, 0x00];
    stream.write_all(&request).await?;

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await?;
    if response != [0x05, 0x00] {
        return Err(anyhow!(
            "SOCKS5 server rejected authentication method (got {:?})",
            response
        ));
    }
    Ok(())
}

async fn request_udp_associate(stream: &mut TcpStream) -> Result<SocketAddr> {
    let mut request = vec![0x05, 0x03, 0x00, 0x01];
    request.extend_from_slice(&[0, 0, 0, 0]); // 0.0.0.0
    request.extend_from_slice(&0u16.to_be_bytes());
    stream.write_all(&request).await?;

    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    if header[1] != 0x00 {
        return Err(anyhow!(
            "SOCKS5 UDP associate failed with reply code {}",
            header[1]
        ));
    }

    let relay_addr = read_socks_address(stream, header[3]).await?;
    Ok(relay_addr)
}

async fn read_socks_address(stream: &mut TcpStream, atyp: u8) -> Result<SocketAddr> {
    let addr = match atyp {
        0x01 => {
            let mut bytes = [0u8; 4];
            stream.read_exact(&mut bytes).await?;
            IpAddr::V4(Ipv4Addr::from(bytes))
        }
        0x04 => {
            let mut bytes = [0u8; 16];
            stream.read_exact(&mut bytes).await?;
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut buf = vec![0u8; len[0] as usize];
            stream.read_exact(&mut buf).await?;
            let hostname = String::from_utf8(buf)
                .map_err(|_| anyhow!("SOCKS5 server returned invalid domain name"))?;
            return Err(anyhow!(
                "SOCKS5 server returned domain {hostname} for UDP relay, which is unsupported"
            ));
        }
        other => {
            return Err(anyhow!("Unsupported ATYP {} in SOCKS5 response", other));
        }
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);
    Ok(SocketAddr::new(addr, port))
}

async fn send_udp_packet(assoc: &mut UdpAssociation, params: &UdpWorkerParams) -> Result<()> {
    let idx = rng().random_range(0..params.targets.len());
    let target = &params.targets[idx];
    let packet = build_udp_packet(target, &params.payload)?;

    assoc
        .udp_socket
        .send_to(&packet, assoc.relay_addr)
        .await
        .map_err(|e| anyhow!("UDP send failed: {e}"))?;
    params.counters.record_packet(params.payload.len());

    if let Some(interval) = params.packet_interval {
        sleep(interval).await;
    }

    Ok(())
}

fn build_udp_packet(target: &SocketTarget, payload: &[u8]) -> Result<Vec<u8>> {
    let mut packet = Vec::with_capacity(payload.len() + target.host.len() + 10);
    packet.extend_from_slice(&[0x00, 0x00]); // RSV
    packet.push(0x00); // FRAG

    if let Ok(ip) = target.host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(addr) => {
                packet.push(0x01);
                packet.extend_from_slice(&addr.octets());
            }
            IpAddr::V6(addr) => {
                packet.push(0x04);
                packet.extend_from_slice(&addr.octets());
            }
        }
    } else {
        if target.host.len() > u8::MAX as usize {
            return Err(anyhow!(
                "Domain {} is too long for SOCKS5 UDP header",
                target.host
            ));
        }
        packet.push(0x03);
        packet.push(target.host.len() as u8);
        packet.extend_from_slice(target.host.as_bytes());
    }

    packet.extend_from_slice(&target.port.to_be_bytes());
    packet.extend_from_slice(payload);
    Ok(packet)
}
