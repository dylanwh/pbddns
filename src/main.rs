#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod config;
mod porkbun;

use axum::{extract::State, routing::get, Json, Router};
use clap::Parser;
use config::Config;
use eyre::Result;
use porkbun::RecordType::A;
use reqwest::{Client, StatusCode};
use std::{collections::HashMap, net::IpAddr, sync::Arc, time::Duration};
use tokio::{sync::Mutex, time};
use tracing_subscriber::{
    filter, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, Layer,
};

type DNSCache = HashMap<String, Vec<IpAddr>>;

#[derive(Debug, Clone)]
struct AppState {
    dns_cache: Arc<Mutex<DNSCache>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            dns_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

async fn update_loop(config: Arc<Config>, client: Client, dns_cache: Arc<Mutex<DNSCache>>) {
    let mut interval = time::interval(Duration::from_secs(60 * 60));

    loop {
        for (name, ips) in config.domains() {
            let mut dns_cache = dns_cache.lock().await;
            let cached_ips = dns_cache.entry(name.clone()).or_default();
            if cached_ips == &ips {
                tracing::info!("{name} has not changed");
                continue;
            }
            *cached_ips = ips.clone();
            update_dns(&client, &config.domain, &name, &ips).await;
        }
        interval.tick().await;
    }
}

async fn update_dns(client: &Client, domain: &str, name: &str, ips: &[IpAddr]) {
    for ip in ips {
        tracing::info!("updating {} to {}", name, ip);
        let params = porkbun::Params {
            domain: domain.to_string(),
            name: name.to_string(),
            record_type: match ip {
                IpAddr::V4(_) => A,
                IpAddr::V6(_) => porkbun::RecordType::AAAA,
            },
            content: ip.to_string(),
            ttl: Some("600".to_string()),
            prio: None,
        };
        if let Ok(id) = porkbun::create_or_edit(client, &params).await {
            tracing::info!("{name} updated to {ip} with id {id}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(atty::is(atty::Stream::Stdout))
                .with_filter(filter::LevelFilter::INFO),
        )
        .init();
    let config = Arc::new(Config::parse());
    let client = reqwest::Client::new();
    let state = AppState::new();

    if config.ping {
        let ip = porkbun::ping(&client).await?;
        println!("Porkbun says your IP is {ip}");
        return Ok(());
    }

    tokio::spawn(update_loop(config.clone(), client, state.dns_cache.clone()));

    let router = Router::new()
        .route("/", get(status))
        .with_state(state);

    tracing::debug!("listening on {}", &config.listen);
    axum::Server::bind(&config.listen)
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

async fn status(State(state): State<AppState>) -> Result<Json<DNSCache>, StatusCode> {
    let dns_cache = state.dns_cache.lock().await;

    Ok(Json(dns_cache.clone()))
}
