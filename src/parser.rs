use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VlessConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub network: String,
    pub security: String,
    pub sni: Option<String>,
    pub flow: Option<String>,
    pub public_key: Option<String>,
    pub short_id: Option<String>,
    pub fingerprint: Option<String>,
    pub header_type: Option<String>,
    pub path: Option<String>,
    pub host_header: Option<String>,
    pub mode: Option<String>,
    pub extra_xhttp: Option<String>,
    pub service_name: Option<String>,
    pub multi_mode: bool,
    pub idle_timeout: Option<i32>,
    pub windows_size: Option<i32>,
    pub allow_insecure: bool,
    pub alpn: Vec<String>,
    pub level: Option<i32>,
    pub raw: String,
}

impl VlessConfig {
    pub fn parse(vless_url: &str) -> Result<Self> {
        if !vless_url.starts_with("vless://") {
            return Err(anyhow!("Invalid VLESS URL: must start with 'vless://'"));
        }

        let url = Url::parse(vless_url).context("Failed to parse VLESS URL")?;

        let id = url.username();
        if id.is_empty() {
            return Err(anyhow!("VLESS URL missing user ID"));
        }

        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("VLESS URL missing host"))?
            .to_string();

        let port = url.port().unwrap_or(443);
        if port == 0 || port == 1 {
            return Err(anyhow!("skipping port: {}", port));
        }

        let params: HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let mut config = VlessConfig {
            id: id.to_string(),
            host,
            port,
            network: params
                .get("type")
                .cloned()
                .unwrap_or_else(|| "tcp".to_string()),
            security: params
                .get("security")
                .cloned()
                .unwrap_or_else(|| "none".to_string()),
            sni: params.get("sni").cloned(),
            flow: params.get("flow").cloned(),
            public_key: params.get("pbk").cloned(),
            short_id: params.get("sid").cloned(),
            fingerprint: params.get("fp").cloned(),
            header_type: params.get("headerType").cloned(),
            path: params.get("path").cloned(),
            host_header: params.get("host").cloned(),
            mode: None,
            extra_xhttp: None,
            service_name: None,
            multi_mode: params
                .get("multiMode")
                .map(|v| v == "true")
                .unwrap_or(false),
            idle_timeout: params
                .get("idleTimeout")
                .and_then(|s| s.parse::<i32>().ok()),
            windows_size: params.get("windowSize").and_then(|s| s.parse::<i32>().ok()),
            allow_insecure: params
                .get("allowInsecure")
                .map(|v| v == "true")
                .unwrap_or(false),
            alpn: params
                .get("alpn")
                .map(|s| s.split(',').map(|x| x.to_string()).collect())
                .unwrap_or_default(),
            level: params.get("level").and_then(|s| s.parse::<i32>().ok()),
            raw: vless_url.to_string(),
        };

        if config.network == "xhttp" {
            config.mode = params.get("mode").cloned();
            if let Some(extra) = params.get("extra") {
                let unquoted = extra.trim_matches('"').to_string();
                config.extra_xhttp = Some(unquoted);
            }
        }

        if config.network == "grpc" {
            config.service_name = params.get("serviceName").cloned();
        }

        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("VLESS config missing ID"));
        }

        if self.host.is_empty() {
            return Err(anyhow!("VLESS config missing host"));
        }

        if self.port == 0 {
            return Err(anyhow!("VLESS config has invalid port"));
        }

        match self.security.as_str() {
            "none" | "tls" | "reality" => {}
            _ => return Err(anyhow!("Unsupported security type: {}", self.security)),
        }

        match self.network.as_str() {
            "tcp" | "ws" | "grpc" | "h2" | "xhttp" | "httpupgrade" => {}
            _ => return Err(anyhow!("Unsupported network type: {}", self.network)),
        }

        if self.security == "reality" {
            if self.public_key.is_none() {
                return Err(anyhow!("Reality security requires public key"));
            }
            if self.short_id.is_none() {
                return Err(anyhow!("Reality security requires short ID"));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrojanConfig {
    pub name: Option<String>,
    pub password: String,
    pub server: String,
    pub port: u16,
    pub security: Option<String>,
    pub network: Option<String>,
    pub flow: Option<String>,
    pub path: Option<String>,
    pub host: Option<String>,
    pub sni: Option<String>,
    pub fingerprint: Option<String>,
    pub allow_insecure: bool,
    pub alpn: Vec<String>,
    pub service_name: Option<String>,
    pub multi_mode: bool,
    pub idle_timeout: Option<i32>,
    pub windows_size: Option<i32>,
    pub settings: HashMap<String, String>,
}

impl TrojanConfig {
    pub fn parse(url_str: &str) -> Result<Self> {
        if !url_str.starts_with("trojan://") {
            return Err(anyhow!("Invalid Trojan URL: must start with 'trojan://'"));
        }
        let u = Url::parse(url_str).context("Failed to parse Trojan URL")?;

        let password = u.username().to_string();
        if password.is_empty() {
            return Err(anyhow!("Trojan URL missing password"));
        }

        let host = u
            .host_str()
            .ok_or_else(|| anyhow!("Trojan URL missing host"))?
            .to_string();
        let port = u.port().ok_or_else(|| anyhow!("Trojan URL missing port"))?;
        if port == 0 || port == 1 {
            return Err(anyhow!("skipping port: {}", port));
        }

        let mut settings: HashMap<String, String> = HashMap::new();
        let qp: HashMap<String, String> = u
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (k, v) in &qp {
            settings.insert(k.clone(), v.clone());
        }

        let config = TrojanConfig {
            name: if u.fragment().unwrap_or("").is_empty() {
                None
            } else {
                Some(u.fragment().unwrap().to_string())
            },
            password,
            server: host,
            port,
            security: qp.get("security").cloned(),
            network: qp.get("type").cloned(),
            flow: qp.get("flow").cloned(),
            path: qp.get("path").cloned(),
            host: qp.get("host").cloned(),
            sni: qp.get("sni").cloned(),
            fingerprint: qp.get("fp").cloned(),
            allow_insecure: qp
                .get("allowInsecure")
                .map(|v| v == "true")
                .unwrap_or(false),
            alpn: qp
                .get("alpn")
                .map(|s| s.split(',').map(|x| x.to_string()).collect())
                .unwrap_or_default(),
            service_name: qp.get("serviceName").cloned(),
            multi_mode: qp.get("multiMode").map(|v| v == "true").unwrap_or(false),
            idle_timeout: qp.get("idleTimeout").and_then(|s| s.parse::<i32>().ok()),
            windows_size: qp.get("windowSize").and_then(|s| s.parse::<i32>().ok()),
            settings,
        };

        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowsocksConfig {
    pub name: Option<String>,
    pub method: String,
    pub password: String,
    pub server: String,
    pub port: u16,
    pub settings: HashMap<String, String>,
}

impl ShadowsocksConfig {
    pub fn parse(url_str: &str) -> Result<Self> {
        if !url_str.starts_with("ss://") {
            return Err(anyhow!("Invalid Shadowsocks URL: must start with 'ss://'"));
        }
        let u = Url::parse(url_str).context("Failed to parse Shadowsocks URL")?;

        let userinfo = if let Some(pw) = u.password() {
            format!("{}:{}", u.username(), pw)
        } else {
            u.username().to_string()
        };
        if userinfo.is_empty() {
            return Err(anyhow!("Shadowsocks URL missing userinfo"));
        }

        let decoded = auto_decode(&userinfo).unwrap_or_else(|_| userinfo.into_bytes());
        let decoded_str = String::from_utf8_lossy(&decoded);

        let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("invalid method:password format"));
        }
        let method = parts[0].to_string();
        let password = parts[1].to_string();

        let server = u
            .host_str()
            .ok_or_else(|| anyhow!("Shadowsocks URL missing host"))?
            .to_string();
        let port = u
            .port()
            .ok_or_else(|| anyhow!("Shadowsocks URL missing port"))?;
        if port == 0 || port == 1 {
            return Err(anyhow!("skipping port: {}", port));
        }

        let mut settings: HashMap<String, String> = HashMap::new();
        for (k, v) in u.query_pairs() {
            settings.insert(k.to_string(), v.to_string());
        }

        Ok(ShadowsocksConfig {
            name: if u.fragment().unwrap_or("").is_empty() {
                None
            } else {
                Some(u.fragment().unwrap().to_string())
            },
            method,
            password,
            server,
            port,
            settings,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "protocol", content = "config")]
pub enum ProxyConfig {
    Vless(VlessConfig),
    Trojan(TrojanConfig),
    Shadowsocks(ShadowsocksConfig),
}

pub fn parse_proxy_url(proxy_url: &str) -> Result<ProxyConfig> {
    let proxy_url = proxy_url.trim();
    if proxy_url.is_empty() {
        return Err(anyhow!("empty proxy URL"));
    }

    let u = Url::parse(proxy_url).context("error parsing proxy URL")?;
    let scheme = u.scheme();
    if scheme.is_empty() {
        return Err(anyhow!("protocol is missing in URL: {}", proxy_url));
    }

    match scheme {
        "vless" => {
            let cfg = VlessConfig::parse(proxy_url)?;
            cfg.validate()?;
            Ok(ProxyConfig::Vless(cfg))
        }
        "trojan" => Ok(ProxyConfig::Trojan(TrojanConfig::parse(proxy_url)?)),
        "ss" => Ok(ProxyConfig::Shadowsocks(ShadowsocksConfig::parse(
            proxy_url,
        )?)),
        _ => Err(anyhow!("unsupported protocol: {}", scheme)),
    }
}

pub fn parse_proxy_list(content: &str) -> Result<Vec<ProxyConfig>> {
    let mut configs = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match parse_proxy_url(line) {
            Ok(cfg) => configs.push(cfg),
            Err(e) => log::warn!("Failed to parse proxy URL on line {}: {}", line_num + 1, e),
        }
    }
    if configs.is_empty() {
        return Err(anyhow!("No valid proxy configurations found"));
    }
    Ok(configs)
}

fn auto_decode(input: &str) -> Result<Vec<u8>> {
    if let Ok(decoded) = percent_decode_str(input).decode_utf8() {
        let s = decoded.to_string();
        if let Ok(bytes) = STANDARD.decode(s.as_bytes()) {
            return Ok(bytes);
        }
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(s.as_bytes()) {
            return Ok(bytes);
        }
        return Ok(s.into_bytes());
    }
    if let Ok(bytes) = STANDARD.decode(input.as_bytes()) {
        return Ok(bytes);
    }
    if let Ok(bytes) = URL_SAFE_NO_PAD.decode(input.as_bytes()) {
        return Ok(bytes);
    }
    Ok(input.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_vless() {
        let url = "vless://user-id@example.com:443?type=tcp&security=none";
        let config = VlessConfig::parse(url).unwrap();

        assert_eq!(config.id, "user-id");
        assert_eq!(config.host, "example.com");
        assert_eq!(config.port, 443);
        assert_eq!(config.network, "tcp");
        assert_eq!(config.security, "none");
    }

    #[test]
    fn test_parse_reality_vless() {
        let url = "vless://uuid@server.domain.com:443?security=reality&sni=server.domain.com&fp=chrome&pbk=public_key&sid=123&spx=/&type=tcp&flow=xtls-rprx-vision&encryption=none#test";
        let config = VlessConfig::parse(url).unwrap();

        assert_eq!(config.security, "reality");
        assert_eq!(config.sni, Some("server.domain.com".to_string()));
        assert_eq!(config.public_key, Some("public_key".to_string()));
        assert_eq!(config.short_id, Some("123".to_string()));
        assert_eq!(config.fingerprint, Some("chrome".to_string()));
        assert_eq!(config.flow, Some("xtls-rprx-vision".to_string()));
        assert_eq!(config.raw, url);
    }

    #[test]
    fn test_invalid_url() {
        let url = "http://example.com";
        assert!(VlessConfig::parse(url).is_err());
    }

    #[test]
    fn test_parse_trojan_basic() {
        let url =
            "trojan://pass@example.com:443?type=grpc&security=tls&sni=example.com&alpn=h2#name";
        let cfg = TrojanConfig::parse(url).unwrap();
        assert_eq!(cfg.password, "pass");
        assert_eq!(cfg.server, "example.com");
        assert_eq!(cfg.port, 443);
        assert_eq!(cfg.network.as_deref(), Some("grpc"));
        assert_eq!(cfg.security.as_deref(), Some("tls"));
        assert_eq!(cfg.sni.as_deref(), Some("example.com"));
        assert_eq!(cfg.alpn, vec!["h2".to_string()]);
        assert_eq!(cfg.name.as_deref(), Some("name"));
    }

    #[test]
    fn test_parse_shadowsocks_basic() {
        // userinfo is method:password
        let url = "ss://aes-128-gcm:secret@example.com:8388#ssnode";
        let cfg = ShadowsocksConfig::parse(url).unwrap();
        assert_eq!(cfg.method, "aes-128-gcm");
        assert_eq!(cfg.password, "secret");
        assert_eq!(cfg.server, "example.com");
        assert_eq!(cfg.port, 8388);
        assert_eq!(cfg.name.as_deref(), Some("ssnode"));
    }

    #[test]
    fn test_parse_proxy_url_vless() {
        let url = "vless://id@host:443?type=tcp&security=none";
        let p = parse_proxy_url(url).unwrap();
        match p {
            ProxyConfig::Vless(v) => {
                assert_eq!(v.id, "id");
                assert_eq!(v.host, "host");
                assert_eq!(v.port, 443);
            }
            _ => panic!("expected Vless"),
        }
    }

    #[test]
    fn test_parse_proxy_url_trojan() {
        let url = "trojan://pass@host:443?security=tls&type=grpc";
        let p = parse_proxy_url(url).unwrap();
        match p {
            ProxyConfig::Trojan(t) => {
                assert_eq!(t.password, "pass");
                assert_eq!(t.server, "host");
                assert_eq!(t.port, 443);
            }
            _ => panic!("expected Trojan"),
        }
    }

    #[test]
    fn test_parse_proxy_url_ss() {
        let url = "ss://chacha20-ietf-poly1305:pwd@host:8388";
        let p = parse_proxy_url(url).unwrap();
        match p {
            ProxyConfig::Shadowsocks(s) => {
                assert_eq!(s.method, "chacha20-ietf-poly1305");
                assert_eq!(s.password, "pwd");
                assert_eq!(s.server, "host");
                assert_eq!(s.port, 8388);
            }
            _ => panic!("expected Shadowsocks"),
        }
    }

    #[test]
    fn test_parse_proxy_url_unsupported() {
        let url = "socks5://localhost:1080";
        assert!(parse_proxy_url(url).is_err());
    }

    #[test]
    fn test_parse_proxy_list_mixed() {
        let content = r#"
            # comment
            vless://id@host:443?type=tcp
            trojan://pass@t.example.com:443?security=tls
            ss://chacha20-ietf-poly1305:pwd@1.2.3.4:8388
            vmess://ignored
        "#;
        let list = parse_proxy_list(content).unwrap();
        assert_eq!(list.len(), 3);
        assert!(
            matches!(list[0], ProxyConfig::Vless(_))
                || matches!(list[1], ProxyConfig::Vless(_))
                || matches!(list[2], ProxyConfig::Vless(_))
        );
        assert!(list.iter().any(|p| matches!(p, ProxyConfig::Trojan(_))));
        assert!(
            list.iter()
                .any(|p| matches!(p, ProxyConfig::Shadowsocks(_)))
        );
    }
}
