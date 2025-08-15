use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "herscat",
    version,
    about = "High-intensity xray proxy stress tester in Rust",
    long_about = "herscat - Launch multiple xray-core instances and run hundreds of parallel HTTP streams via SOCKS5 proxies for load/stress testing of proxy setups."
)]
pub struct Args {
    /// Proxy URL to use for connection (supports vless/trojan/ss)
    #[arg(short = 'u', long, value_name = "PROXY_URL")]
    pub url: Option<String>,

    /// File containing list of proxy URLs (one per line)
    #[arg(short = 'l', long, value_name = "FILE")]
    pub list: Option<String>,

    // Removed threads: we use a single total concurrency value
    /// Duration to run the test in seconds (0 = infinite)
    #[arg(short = 'd', long, default_value_t = 0)]
    pub duration: u64,

    /// Number of xray-core instances to launch
    #[arg(short = 'x', long = "instances", default_value_t = 5)]
    pub xray_instances: usize,

    /// Base port for SOCKS5 proxies (incremented for each instance)
    #[arg(short = 'p', long = "base-port", default_value_t = 10808)]
    pub base_port: u16,

    /// Total concurrency (number of simultaneous downloads across all instances)
    #[arg(short = 'c', long = "concurrency", default_value_t = 200)]
    pub concurrency: usize,

    /// Custom target URLs for stress testing (comma-separated)
    #[arg(long = "targets", value_name = "URLS")]
    pub custom_targets: Option<String>,

    /// Enable verbose logging
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Enable debug mode
    #[arg(long = "debug", action = clap::ArgAction::SetTrue)]
    pub debug: bool,

    /// Statistics reporting interval in seconds
    #[arg(long = "stats-interval", default_value_t = 5)]
    pub stats_interval: u64,

    #[command(subcommand)]
    pub cmd: Option<Commands>,
}

impl Args {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.url.is_none() && self.list.is_none() {
            return Err(anyhow::anyhow!("Either --url or --list must be provided"));
        }

        if self.url.is_some() && self.list.is_some() {
            return Err(anyhow::anyhow!(
                "Cannot specify both --url and --list, choose one"
            ));
        }

        if self.xray_instances == 0 {
            return Err(anyhow::anyhow!("Xray instances must be greater than 0"));
        }

        if self.concurrency == 0 {
            return Err(anyhow::anyhow!("Concurrency must be greater than 0"));
        }

        Ok(())
    }
}
