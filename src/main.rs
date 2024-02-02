#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used
)]
#![allow(clippy::redundant_pub_crate)]

mod config;
mod porkbun;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use config::Config;
use eyre::Result;
use futures::future::join_all;
use porkbun::RecordType::A;
use reqwest::Client;
use std::{collections::HashMap, io::IsTerminal, net::IpAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, signal, sync::Mutex, time};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing_subscriber::{
    filter, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, Layer,
};

type DNSCache = HashMap<String, Vec<IpAddr>>;

#[derive(Debug, Clone)]
struct AppState {
    dns_cache: Arc<Mutex<DNSCache>>,
    config: Arc<Config>,
    client: Client,
}

impl AppState {
    fn new(config: Arc<Config>, client: Client) -> Self {
        Self {
            dns_cache: Arc::new(Mutex::new(HashMap::new())),
            config,
            client,
        }
    }
}

async fn update_loop(
    shutdown: CancellationToken,
    config: Arc<Config>,
    client: Client,
    dns_cache: Arc<Mutex<DNSCache>>,
) {
    let mut interval = time::interval(Duration::from_secs(60 * 60));

    loop {
        update_once(config.clone(), client.clone(), Some(dns_cache.clone())).await;
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => continue,
        }
    }
}

async fn update_once(config: Arc<Config>, client: Client, dns_cache: Option<Arc<Mutex<DNSCache>>>) {
    let mut handles = vec![];
    for (name, ips) in config.domains() {
        if let Some(ref dns_cache) = dns_cache {
            let mut dns_cache = dns_cache.lock().await;
            let cached_ips = dns_cache.entry(name.clone()).or_default();
            if cached_ips == &ips {
                tracing::info!("{name} has not changed ({ips:?})");
                continue;
            }
            *cached_ips = ips.clone();
            drop(dns_cache);
        }
        for ip in ips {
            let j = tokio::spawn(update_dns(
                client.clone(),
                config.domain.clone(),
                name.clone(),
                ip,
            ));
            handles.push(j);
        }
    }
    join_all(handles).await;
}

async fn update_dns(client: Client, domain: String, name: String, ip: IpAddr) {
    tracing::info!("checking {} to {}", name, ip);
    let params = porkbun::Params {
        domain,
        name,
        record_type: match ip {
            IpAddr::V4(_) => A,
            IpAddr::V6(_) => porkbun::RecordType::AAAA,
        },
        content: ip.to_string(),
        ttl: Some("600".to_string()),
        prio: None,
    };
    let r = porkbun::create_or_edit(&client, &params).await;
    match r {
        Ok((id, true)) => {
            tracing::info!("Record {id} updated");
        }
        Ok((id, false)) => {
            tracing::debug!("Record {id} already up to date");
        }
        Err(err) => tracing::error!("Failed to update record: {err}"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(std::io::stdout().is_terminal())
                .with_filter(filter::LevelFilter::INFO),
        )
        .init();
    let config = Arc::new(Config::parse());
    let client = reqwest::Client::new();
    let state = AppState::new(config.clone(), client.clone());

    if config.ping {
        let ip = porkbun::ping(&client).await?;
        println!("Porkbun says your IP is {ip}");
        return Ok(());
    }

    if config.once {
        update_once(config.clone(), client.clone(), None).await;
        return Ok(());
    }

    if let Some(write_pid) = &config.write_pid {
        std::fs::write(write_pid, std::process::id().to_string())?;
    }
    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();

    tracker.spawn(update_loop(
        shutdown.clone(),
        config.clone(),
        client,
        state.dns_cache.clone(),
    ));

    let router = Router::new()
        .route("/", get(status))
        .route("/refresh", post(refresh))
        .with_state(state);

    tracing::debug!("listening on {}", &config.listen);
    let listener = TcpListener::bind(&config.listen).await?;
    let serve_shutdown = shutdown.clone();
    tracker.spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move { serve_shutdown.cancelled().await })
            .await
            .unwrap_or_else(|e| {
                tracing::error!("server error: {e}");
            });
    });
    tracker.close();
    shutdown_signal().await;
    shutdown.cancel();
    tracker.wait().await;

    Ok(())
}

async fn refresh(State(state): State<AppState>) -> Result<Json<DNSCache>, StatusCode> {
    let config = state.config;
    let client = state.client;
    let dns_cache = state.dns_cache;

    tracing::info!("refreshing");
    update_once(config, client, Some(dns_cache.clone())).await;

    let dns_cache = dns_cache.lock().await;
    Ok(Json(dns_cache.clone()))
}

async fn status(State(state): State<AppState>) -> Result<Json<DNSCache>, StatusCode> {
    let dns_cache = state.dns_cache.lock().await;

    Ok(Json(dns_cache.clone()))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        #[allow(clippy::expect_used)]
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        #[allow(clippy::expect_used)]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
