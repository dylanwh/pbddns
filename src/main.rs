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
use futures::future::join_all;

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

async fn update_loop(config: Arc<Config>, client: Arc<Client>, dns_cache: Arc<Mutex<DNSCache>>) {
    let mut interval = time::interval(Duration::from_secs(60 * 60));

    loop {
        let domain = &config.domain;
        for (name, ips) in config.domains() {
            let mut dns_cache = dns_cache.lock().await;
            let cached_ips = dns_cache.entry(name.clone()).or_default();
            if cached_ips == &ips {
                tracing::info!("{name} has not changed ({ips:?})");
                continue;
            }
            *cached_ips = ips.clone();
            drop(dns_cache);
            for ip in ips {
                tokio::spawn(update_dns(client.clone(), domain.clone(), name.clone(), ip));
            }
        }
        interval.tick().await;
    }
}

async fn update_once(config: Arc<Config>, client: Arc<Client>) {
    let mut handles = vec![];
    for (name, ips) in config.domains() {
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

async fn update_dns(client: Arc<Client>, domain: String, name: String, ip: IpAddr) {
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
                .with_ansi(atty::is(atty::Stream::Stdout))
                .with_filter(filter::LevelFilter::INFO),
        )
        .init();
    let config = Arc::new(Config::parse());
    let client = Arc::new(reqwest::Client::new());
    let state = AppState::new();

    if config.ping {
        let ip = porkbun::ping(&client).await?;
        println!("Porkbun says your IP is {ip}");
        return Ok(());
    }

    if config.once {
        update_once(config.clone(), client.clone()).await;
        return Ok(());
    }

    tokio::spawn(update_loop(config.clone(), client, state.dns_cache.clone()));

    let router = Router::new().route("/", get(status)).with_state(state);

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
