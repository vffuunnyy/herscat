use crate::config::ConfigGenerator;
use crate::parser::ProxyConfig;
use anyhow::{Context, Result};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct XrayInstance {
    pub port: u16,
    pub process: Child,
}

impl XrayInstance {
    pub fn new(
        proxy_config: &ProxyConfig,
        port: u16,
        config_generator: &ConfigGenerator,
    ) -> Result<Self> {
        let config_path = config_generator.generate_config(proxy_config, port)?;

        log::info!(
            "Starting xray-core instance on port {} with config: {}",
            port,
            config_path.display()
        );

        let mut process = Command::new("xray")
            .arg("-c")
            .arg(&config_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start xray-core process for port {port}"))?;

        match process.try_wait() {
            Ok(Some(status)) => {
                return Err(anyhow::anyhow!(
                    "xray-core process exited immediately with status: {}",
                    status
                ));
            }
            Ok(None) => {
                log::info!(
                    "xray-core started successfully (PID: {}) on port {}",
                    process.id(),
                    port
                );
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to check xray-core process status: {}",
                    e
                ));
            }
        }

        Ok(XrayInstance { port, process })
    }

    pub fn is_running(&mut self) -> bool {
        match self.process.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        }
    }

    pub fn terminate(&mut self) -> Result<()> {
        if self.is_running() {
            log::info!(
                "Terminating xray-core instance (PID: {}) on port {}",
                self.process.id(),
                self.port
            );

            self.process
                .kill()
                .context("Failed to kill xray-core process")?;

            self.process
                .wait()
                .context("Failed to wait for xray-core process termination")?;
        }
        Ok(())
    }
}

impl Drop for XrayInstance {
    fn drop(&mut self) {
        if let Err(e) = self.terminate() {
            log::warn!(
                "Failed to terminate xray instance on port {}: {}",
                self.port,
                e
            );
        }
    }
}

#[derive(Clone)]
pub struct ProcessManager {
    instances: Arc<Mutex<Vec<XrayInstance>>>,
    config_generator: Arc<ConfigGenerator>,
}

impl ProcessManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            instances: Arc::new(Mutex::new(Vec::new())),
            config_generator: Arc::new(ConfigGenerator::new()?),
        })
    }

    fn is_port_available(port: u16) -> bool {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                drop(listener);
                true
            }
            Err(_) => false,
        }
    }

    fn find_next_free_port(mut start_port: u16) -> Option<u16> {
        for _ in 0..10_000u32 {
            if Self::is_port_available(start_port) {
                return Some(start_port);
            }
            if start_port == u16::MAX {
                break;
            }
            start_port = start_port.saturating_add(1);
        }
        None
    }

    pub async fn start_instances(
        &self,
        proxy_configs: &[ProxyConfig],
        base_port: u16,
        num_instances: usize,
    ) -> Result<Vec<u16>> {
        let mut instances = self.instances.lock().await;
        let mut ports = Vec::new();

        log::info!("Starting {num_instances} xray-core instances from base port {base_port}");

        let mut probe_port = base_port;
        for i in 0..num_instances {
            let port = match Self::find_next_free_port(probe_port) {
                Some(p) => p,
                None => {
                    log::error!("No free port found starting from {probe_port} for instance {i}");
                    break;
                }
            };
            probe_port = port.saturating_add(1);
            let proxy_config = &proxy_configs[i % proxy_configs.len()];

            match XrayInstance::new(proxy_config, port, &self.config_generator) {
                Ok(instance) => {
                    ports.push(port);
                    instances.push(instance);
                }
                Err(e) => {
                    log::error!("Failed to start xray instance on port {port}: {e}");
                }
            }
        }

        if ports.is_empty() {
            return Err(anyhow::anyhow!("Failed to start any xray-core instances"));
        }

        log::info!("Successfully started {} xray-core instances", ports.len());
        Ok(ports)
    }

    pub async fn terminate_all(&self) -> Result<()> {
        let mut instances = self.instances.lock().await;

        log::info!("Terminating all xray-core instances");

        for instance in instances.iter_mut() {
            if let Err(e) = instance.terminate() {
                log::warn!(
                    "Failed to terminate instance on port {}: {}",
                    instance.port,
                    e
                );
            }
        }

        instances.clear();

        if let Err(e) = self.config_generator.cleanup_all() {
            log::warn!("Failed to cleanup config files: {e}");
        }

        Ok(())
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        if let Ok(mut instances) = self.instances.try_lock() {
            for instance in instances.iter_mut() {
                let _ = instance.terminate();
            }
        }
    }
}
