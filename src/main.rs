mod cli;
mod config;
mod parser;
mod process;
mod stressor;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::{Generator, generate};
use colored::*;
use std::fs;
use std::time::Duration;
use tokio::signal;

use cli::{Args, Commands};
use parser::{ProxyConfig, parse_proxy_list, parse_proxy_url};
use process::ProcessManager;
use stressor::{StressConfig, StressRunner, get_default_targets, parse_custom_targets};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(cmd) = args.cmd {
        match cmd {
            Commands::Completions { shell } => {
                print_completions(shell, &mut Args::command());
                return Ok(());
            }
        }
    }

    let log_level = match (args.debug, args.verbose) {
        (true, _) => "debug",
        (false, true) => "info",
        _ => "warn",
    };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();
    args.validate().context("Invalid command line arguments")?;

    print_banner();

    let proxy_configs = load_proxy_configs(&args)
        .await
        .context("Failed to load proxy configurations")?;

    log::info!(
        "Loaded proxies - VLESS: {}, Trojan: {}, SS: {}",
        proxy_configs
            .iter()
            .filter(|p| matches!(p, ProxyConfig::Vless(_)))
            .count(),
        proxy_configs
            .iter()
            .filter(|p| matches!(p, ProxyConfig::Trojan(_)))
            .count(),
        proxy_configs
            .iter()
            .filter(|p| matches!(p, ProxyConfig::Shadowsocks(_)))
            .count()
    );

    let process_manager = ProcessManager::new().context("Failed to initialize process manager")?;
    let proxy_ports = process_manager
        .start_instances(&proxy_configs, args.base_port, args.xray_instances)
        .await
        .context("Failed to start xray-core instances")?;

    if proxy_ports.is_empty() {
        return Err(anyhow::anyhow!(
            "No xray-core instances started successfully"
        ));
    }

    log::info!(
        "Started {} xray-core instances on ports: {:?}",
        proxy_ports.len(),
        proxy_ports
    );

    process_manager.start_monitor(Duration::from_secs(2));

    tokio::time::sleep(Duration::from_secs(3)).await;
    log::info!("Monitor started, proceeding with stress test...");

    let targets = args
        .custom_targets
        .as_ref()
        .map(|target| parse_custom_targets(target))
        .unwrap_or_else(get_default_targets);

    let stress_config = StressConfig {
        targets,
        concurrency: args.concurrency,
        duration: (args.duration > 0).then(|| Duration::from_secs(args.duration)),
        proxy_ports: proxy_ports.clone(),
    };

    let stress_runner =
        StressRunner::new(stress_config.clone()).context("Failed to initialize stress runner")?;

    stress_runner
        .start_stats_reporter(Duration::from_secs(args.stats_interval))
        .await;

    let process_manager_clone = process_manager.clone();
    let stress_runner_clone = stress_runner.clone();

    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                println!(
                    "\n{}",
                    "Received Ctrl+C, shutting down gracefully...".yellow()
                );
                print_stats(&stress_runner_clone);
                if let Err(e) = process_manager_clone.terminate_all().await {
                    log::error!("Error during shutdown: {e}");
                }
                std::process::exit(0);
            }
            Err(err) => {
                log::error!("Unable to listen for shutdown signal: {err}");
            }
        }
    });

    println!(
        "\n{} Starting stress test with total concurrency = {} across {} xray instances",
        "[herscat]".red().bold(),
        args.concurrency.to_string().cyan(),
        proxy_ports.len().to_string().cyan(),
    );

    if let Some(duration) = stress_config.duration {
        println!(
            "{} Test will run for {} seconds",
            "[herscat]".red().bold(),
            duration.as_secs().to_string().cyan()
        );
    } else {
        println!(
            "{} Test will run indefinitely (Ctrl+C to stop)",
            "[herscat]".red().bold()
        );
    }

    stress_runner.run().await.context("Stress test failed")?;

    print_stats(&stress_runner);

    process_manager
        .terminate_all()
        .await
        .context("Failed to cleanup xray processes")?;

    println!(
        "\n{} Test completed successfully!",
        "[herscat]".red().bold()
    );

    Ok(())
}

async fn load_proxy_configs(args: &Args) -> Result<Vec<ProxyConfig>> {
    if let Some(ref url) = args.url {
        let cfg = parse_proxy_url(url).context("Failed to parse proxy URL")?;
        Ok(vec![cfg])
    } else if let Some(ref list_file) = args.list {
        let content = fs::read_to_string(list_file)
            .with_context(|| format!("Failed to read proxy list file: {list_file}"))?;
        parse_proxy_list(&content).context("Failed to parse proxy list")
    } else {
        unreachable!("Either url or list should be provided (validated earlier)")
    }
}

fn print_completions<G: Generator>(generator: G, cmd: &mut clap::Command) {
    generate(
        generator,
        cmd,
        cmd.get_name().to_string(),
        &mut std::io::stdout(),
    );
}

fn print_stats(stress_runner: &StressRunner) {
    log::debug!(
        "About to get final stats - Success: {}, Failed: {}, Bytes: {}",
        stress_runner
            .successful_requests
            .load(std::sync::atomic::Ordering::Relaxed),
        stress_runner
            .failed_requests
            .load(std::sync::atomic::Ordering::Relaxed),
        stress_runner
            .bytes_downloaded
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    let final_stats = stress_runner.get_current_stats();
    log::debug!(
        "Final stats object - Success: {}, Failed: {}, Bytes: {}",
        final_stats.successful_requests,
        final_stats.failed_requests,
        final_stats.bytes_downloaded
    );
    println!("\n{} Final Statistics:", "[herscat]".red().bold());
    println!(
        "  Total Traffic: {} MB",
        format!(
            "{:.2}",
            final_stats.bytes_downloaded as f64 / (1024.0 * 1024.0)
        )
        .cyan()
    );
    println!(
        "  Average Bandwidth: {} Mbps",
        format!(
            "{:.2}",
            (final_stats.bytes_per_second() * 8.0) / (1000.0 * 1000.0)
        )
        .cyan()
    );
    println!(
        "  Test Duration: {}s",
        format!("{:.2}", final_stats.elapsed().as_secs_f64()).cyan()
    );
}

fn print_banner() {
    let art = r#"
         ██╗  ██╗███████╗██████╗ ███████╗ ██████╗ █████╗ ████████╗
         ██║  ██║██╔════╝██╔══██╗██╔════╝██╔════╝██╔══██╗╚══██╔══╝
         ███████║█████╗  ██████╔╝███████╗██║     ███████║   ██║   
         ██╔══██║██╔══╝  ██╔══██╗╚════██║██║     ██╔══██║   ██║   
         ██║  ██║███████╗██║  ██║███████║╚██████╗██║  ██║   ██║   
         ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚══════╝ ╚═════╝╚═╝  ╚═╝   ╚═╝   
    "#;

    let paws = "paws".red().bold();
    let up_for = "up for".red().bold();
    let testing = "testing".red().bold();
    let proxy = "proxy".red().bold();
    let fun = "fun!".yellow().bold();
    let slogan = "\"Meow-xing\" your proxy!".blue().bold();
    let formatted = format!(
        r#"                                   /\__/\
                    /\___/\       /      \
                   (  o   o  )   (  ^   ^  )
                    \   ^   /     \  ___  /
                     ) --- (       ) --- (
                ____/       \     /       \____
           ,,,''   /    /\_/\  \_/   \_/\    \ ',,,,
         ,''      (     \ o o /        \ o o /     )  '',
        '         |\    |  ^  |   /\   |  ^  |    /|     '
                  | |   |  -  |  (  )  |  -  |   | |
                  | |  /\_____/   \/   \_____/\  | |
                 / /  (   ___  )       (  ___   ) \ \
                ( (    \ |o o| /        \ |o o| /   ) )
                 \ \    | ^^^ |          | ^^^ |   / /
                  \_\   |_____|   {paws}   |_____|  /_/
                    |   | | | |  {up_for}  | | | |  |
                    |   |_|_|_|  {testing} |_|_|_|  |
                   /              {proxy}            \
                  (                 {fun}            )
                 /                                   \
               ,-'                                     '-,
             ,'                                           ',
            (           {slogan}            )
             ',                                           ,'
               '-,_                                   _,-'
                   '---,,,__               __,,,---'
                            '""'-------'""'"#
    );
    println!("{}", art.red().bold());
    println!("{}", formatted.green());

    println!(
        "{} {}",
        "herscat".red().bold(),
        "- High-intensity proxy stress tester".white()
    );

    println!(
        "{} {}",
        "Warning:".yellow().bold(),
        "For controlled load testing of proxy setups only.".yellow()
    );
}
