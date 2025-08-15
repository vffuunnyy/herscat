use crate::parser::{ProxyConfig, TrojanConfig, VlessConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XrayConfig {
    pub inbounds: Vec<Value>,
    pub outbounds: Vec<Value>,
}

pub struct ConfigGenerator {
    temp_dir: PathBuf,
}

impl ConfigGenerator {
    pub fn new() -> Result<Self> {
        let temp_dir = std::env::temp_dir().join("herscat_configs");
        fs::create_dir_all(&temp_dir).context("Failed to create temporary config directory")?;

        Ok(Self { temp_dir })
    }

    pub fn generate_config(&self, proxy_config: &ProxyConfig, port: u16) -> Result<PathBuf> {
        let config = self.build_xray_config(proxy_config, port)?;
        let config_path = self.temp_dir.join(format!("config_{port}.json"));

        let config_json =
            serde_json::to_string_pretty(&config).context("Failed to serialize xray config")?;

        fs::write(&config_path, config_json).context("Failed to write config file")?;

        log::debug!("Generated xray config: {}", config_path.display());
        Ok(config_path)
    }

    fn build_xray_config(&self, proxy_config: &ProxyConfig, port: u16) -> Result<XrayConfig> {
        let inbound = serde_json::json!({
            "port": port,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": {
                "auth": "noauth"
            }
        });
        let outbound = match proxy_config {
            ProxyConfig::Vless(v) => {
                let stream_settings = self.build_vless_trojan_stream_settings(Some(v), None)?;
                serde_json::json!({
                    "protocol": "vless",
                    "tag": "vless-out",
                    "settings": {
                        "vnext": [{
                            "address": v.host,
                            "port": v.port,
                            "users": [{
                                "id": v.id,
                                "encryption": "none",
                                "flow": v.flow.as_deref().unwrap_or("")
                            }]
                        }]
                    },
                    "streamSettings": stream_settings
                })
            }
            ProxyConfig::Trojan(t) => {
                let stream_settings = self.build_vless_trojan_stream_settings(None, Some(t))?;
                serde_json::json!({
                    "protocol": "trojan",
                    "tag": "trojan-out",
                    "settings": {
                        "servers": [{
                            "address": t.server,
                            "port": t.port,
                            "password": t.password
                        }]
                    },
                    "streamSettings": stream_settings
                })
            }
            ProxyConfig::Shadowsocks(s) => {
                serde_json::json!({
                    "protocol": "shadowsocks",
                    "tag": "ss-out",
                    "settings": {
                        "servers": [{
                            "address": s.server,
                            "port": s.port,
                            "method": s.method,
                            "password": s.password
                        }]
                    }
                })
            }
        };

        Ok(XrayConfig {
            inbounds: vec![inbound],
            outbounds: vec![outbound],
        })
    }

    fn build_vless_trojan_stream_settings(
        &self,
        vless: Option<&VlessConfig>,
        trojan: Option<&TrojanConfig>,
    ) -> Result<Value> {
        // Determine common fields
        let (network, security, sni, public_key, short_id, fingerprint) = if let Some(v) = vless {
            (
                v.network.as_str(),
                v.security.as_str(),
                v.sni.clone(),
                v.public_key.clone(),
                v.short_id.clone(),
                v.fingerprint.clone(),
            )
        } else if let Some(t) = trojan {
            (
                t.network.as_deref().unwrap_or("tcp"),
                t.security.as_deref().unwrap_or("none"),
                t.sni.clone(),
                None,
                None,
                t.fingerprint.clone(),
            )
        } else {
            ("tcp", "none", None, None, None, None)
        };

        let mut stream_settings = serde_json::json!({
            "network": network,
            "security": security
        });

        match security {
            "tls" => {
                let mut tls_settings = serde_json::json!({
                    "allowInsecure": true
                });
                if let Some(name) = sni {
                    tls_settings["serverName"] = serde_json::Value::String(name);
                }
                stream_settings["tlsSettings"] = tls_settings;
            }
            "reality" => {
                if let Some(v) = vless {
                    let reality_settings = serde_json::json!({
                        "serverName": v.sni.as_ref().unwrap_or(&v.host),
                        "publicKey": public_key.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Reality requires public key"))?,
                        "shortId": short_id.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Reality requires short ID"))?,
                        "fingerprint": fingerprint.as_ref().unwrap_or(&"chrome".to_string())
                    });
                    stream_settings["realitySettings"] = reality_settings;
                } else {
                    return Err(anyhow::anyhow!(
                        "Reality security is only supported for VLESS"
                    ));
                }
            }
            "none" => {}
            other => return Err(anyhow::anyhow!("Unsupported security type: {}", other)),
        }

        Ok(stream_settings)
    }

    pub fn cleanup_all(&self) -> Result<()> {
        if self.temp_dir.exists() {
            fs::remove_dir_all(&self.temp_dir)
                .context("Failed to cleanup temporary config directory")?;
            log::debug!("Cleaned up all configs in: {}", self.temp_dir.display());
        }
        Ok(())
    }
}

impl Drop for ConfigGenerator {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup_all() {
            log::warn!("Failed to cleanup configs on drop: {e}");
        }
    }
}
