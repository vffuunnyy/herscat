use crate::parser::{ProxyConfig, TrojanConfig, VlessConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
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
                "auth": "noauth",
                "udp": true,
                "ip": "127.0.0.1"
            }
        });
        let outbound = match proxy_config {
            ProxyConfig::Vless(v) => {
                let v = v.as_ref();
                let stream_settings = self.build_vless_trojan_stream_settings(Some(v), None)?;

                let mut user: Map<String, Value> = Map::new();
                user.insert("id".to_string(), serde_json::json!(v.id));
                user.insert(
                    "encryption".to_string(),
                    serde_json::json!(v.encryption.clone()),
                );

                if let Some(flow) = &v.flow && !flow.is_empty() {
                    user.insert("flow".to_string(), serde_json::json!(flow));
                }

                if let Some(level) = v.level {
                    user.insert("level".to_string(), serde_json::json!(level));
                }

                if let Some(packet_encoding) = &v.packet_encoding && !packet_encoding.is_empty() {
                    user.insert(
                        "packetEncoding".to_string(),
                        serde_json::json!(packet_encoding),
                    );
                }

                if let Some(xor_mode) = v.xor_mode {
                    user.insert("xorMode".to_string(), serde_json::json!(xor_mode));
                }

                if let Some(seconds) = v.seconds {
                    user.insert("seconds".to_string(), serde_json::json!(seconds));
                }

                if let Some(padding) = &v.padding && !padding.is_empty() {
                    user.insert("padding".to_string(), serde_json::json!(padding));
                }

                if let Some(tag) = &v.reverse_tag && !tag.is_empty() {
                    user.insert("reverse".to_string(), serde_json::json!({ "tag": tag }));
                }

                let users = Value::Array(vec![Value::Object(user)]);

                serde_json::json!({
                    "protocol": "vless",
                    "tag": "vless-out",
                    "settings": {
                        "vnext": [{
                            "address": v.host,
                            "port": v.port,
                            "users": users
                        }]
                    },
                    "streamSettings": stream_settings
                })
            }
            ProxyConfig::Trojan(t) => {
                let t = t.as_ref();
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
        let (network, security, _, public_key, short_id, fingerprint) = if let Some(v) = vless {
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
                let (allow_insecure, server_name, fp) = if let Some(v) = vless {
                    (
                        v.allow_insecure,
                        v.sni.clone().unwrap_or_else(|| v.host.clone()),
                        v.fingerprint.clone(),
                    )
                } else if let Some(t) = trojan {
                    (
                        t.allow_insecure,
                        t.sni.clone().unwrap_or_else(|| t.server.clone()),
                        t.fingerprint.clone(),
                    )
                } else {
                    (false, String::new(), None)
                };

                let mut tls_settings = serde_json::json!({
                    "allowInsecure": allow_insecure
                });

                if !server_name.is_empty() {
                    tls_settings["serverName"] = serde_json::Value::String(server_name);
                }
                if let Some(fp) = fp {
                    tls_settings["fingerprint"] = serde_json::Value::String(fp);
                }

                stream_settings["tlsSettings"] = tls_settings;
            }
            "reality" => {
                if let Some(v) = vless {
                    let mut reality_settings = serde_json::json!({
                        "serverName": v.sni.as_ref().unwrap_or(&v.host),
                        "publicKey": public_key.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Reality requires public key"))?,
                        "shortId": short_id.as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Reality requires short ID"))?,
                        "fingerprint": fingerprint.as_ref().unwrap_or(&"chrome".to_string())
                    });

                    if let Some(spider) = &v.spider_x
                        && let Value::Object(obj) = &mut reality_settings
                    {
                        obj.insert("spiderX".to_string(), serde_json::json!(spider));
                    }

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

        match network {
            "ws" => {
                if let Some(v) = vless {
                    let mut ws = serde_json::json!({});
                    if let Some(p) = &v.path {
                        ws["path"] = serde_json::Value::String(p.clone());
                    }
                    if let Some(h) = &v.host_header {
                        ws["headers"] = serde_json::json!({ "Host": h });
                    }
                    stream_settings["wsSettings"] = ws;
                } else if let Some(t) = trojan {
                    let mut ws = serde_json::json!({});
                    if let Some(p) = &t.path {
                        ws["path"] = serde_json::Value::String(p.clone());
                    }
                    if let Some(h) = &t.host {
                        ws["headers"] = serde_json::json!({ "Host": h });
                    }
                    stream_settings["wsSettings"] = ws;
                }
            }
            "grpc" => {
                if let Some(v) = vless {
                    if let Some(name) = &v.service_name {
                        stream_settings["grpcSettings"] = serde_json::json!({
                            "serviceName": name
                        });
                    }
                } else if let Some(t) = trojan
                    && let Some(name) = &t.service_name
                {
                    stream_settings["grpcSettings"] = serde_json::json!({
                        "serviceName": name
                    });
                }
            }
            _ => {}
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
