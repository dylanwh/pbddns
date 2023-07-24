use eyre::{eyre, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::IpAddr;

#[allow(clippy::upper_case_acronyms)]
#[derive(
    Debug, Serialize, Deserialize, strum_macros::Display, Default, Copy, Clone, Eq, PartialEq,
)]
pub enum RecordType {
    #[default]
    A,
    MX,
    CNAME,
    ALIAS,
    TXT,
    NS,
    AAAA,
    SRV,
    TLSA,
    CAA,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
struct Record {
    content: String,
    id: String,
    name: String,
    prio: String,
    ttl: String,

    #[serde(rename = "type")]
    record_type: RecordType,

    #[serde(skip)]
    domain: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Params {
    pub domain: String,
    pub record_type: RecordType,
    pub name: String,
    pub content: String,
    pub ttl: Option<String>,
    pub prio: Option<String>,
}

impl Record {
    fn is_modified(&self, params: &Params) -> bool {
        let modified = self.name != params.name
            || self.record_type != params.record_type
            || self.content != params.content
            || self.ttl != params.ttl.clone().unwrap_or_else(|| "600".to_string())
            || self.prio != params.prio.clone().unwrap_or_else(|| "0".to_string());
        tracing::debug!("is_modified( {:#?}, {:#?} ) -> {}", self, params, modified);
        modified
    }

    fn modify(self, params: &Params) -> Result<Self> {
        if self.domain.is_none() {
            return Err(eyre!("no domain on record"));
        }
        if self.domain != Some(params.domain.clone()) {
            return Err(eyre!(
                "record domain {} does not match params domain {}",
                self.domain.unwrap_or_default(),
                params.domain
            ));
        }

        let r = Self {
            name: params.name.clone(),
            record_type: params.record_type,
            content: params.content.clone(),
            ttl: params.ttl.clone().unwrap_or(self.ttl),
            prio: params.prio.clone().unwrap_or(self.prio),
            ..self
        };
        Ok(r)
    }
}

const PORKBUN_API: &str = "https://porkbun.com/api/json/v3";

fn api_key() -> Result<String> {
    Ok(std::env::var("PORKBUN_API_KEY")?)
}

fn secret_key() -> Result<String> {
    Ok(std::env::var("PORKBUN_SECRET_KEY")?)
}

pub async fn ping(client: &Client) -> Result<IpAddr> {
    let resp = client
        .post(format!("{PORKBUN_API}/ping"))
        .json(&json!({
            "apikey": api_key()?,
            "secretapikey": secret_key()?,
        }))
        .send()
        .await?;

    let value = validate_json(resp.json::<Value>().await?)?;
    let ip = value["yourIp"]
        .as_str()
        .ok_or_else(|| eyre!("no yourIp field on ping response"))?;
    Ok(ip.parse()?)
}

async fn retrieve_by_name_type(client: &Client, params: &Params) -> Result<Vec<Record>> {
    let domain = &params.domain;
    let name = &params.name;
    let record_type = params.record_type;
    let url = format!("{PORKBUN_API}/dns/retrieveByNameType/{domain}/{record_type}/{name}");
    tracing::debug!("retrieve_by_name_type url: {url}");
    let resp = client
        .post(&url)
        .json(&json!({
            "apikey": api_key()?,
            "secretapikey": secret_key()?,
        }))
        .send()
        .await?;
    let value = validate_json(resp.json::<Value>().await?)?;
    let records: Vec<Record> = serde_json::from_value(value["records"].clone())?;
    let records = records
        .into_iter()
        .map(|r| {
            let suffix = format!(".{domain}");
            let domain = domain.to_string();
            Record {
                domain: Some(domain),
                name: r
                    .name
                    .to_string()
                    .strip_suffix(&suffix)
                    .unwrap_or(&r.name)
                    .to_string(),
                ..r
            }
        })
        .collect();

    Ok(records)
}

pub async fn create(client: &Client, params: &Params) -> Result<String> {
    let domain = &params.domain;
    let url = format!("{PORKBUN_API}/dns/create/{domain}");
    tracing::debug!("create url: {url}");
    let body = json!({
        "apikey": api_key()?,
        "secretapikey": secret_key()?,
        "content": params.content,
        "name": params.name,
        "prio": params.prio,
        "ttl": params.ttl,
        "type": params.record_type,
    });
    tracing::debug!("create body: {:#?}", body);
    let resp = client.post(url).json(&body).send().await?;
    let value = validate_json(resp.json::<Value>().await?)?;
    let id = value["id"].clone();
    if let Some(id) = id.as_str() {
        return Ok(id.to_string());
    }
    if let Some(id) = id.as_u64() {
        return Ok(id.to_string());
    }
    Err(eyre!("no id in response: {:#?}", value))
}

async fn edit(client: &Client, record: Record) -> Result<()> {
    let domain = record
        .domain
        .as_ref()
        .ok_or(eyre!("record has no domain"))?;
    let url = format!("{PORKBUN_API}/dns/edit/{domain}/{id}", id = record.id);
    tracing::debug!("edit url: {url}");
    let body = json!({
            "apikey": api_key()?,
            "secretapikey": secret_key()?,
            "name": record.name,
            "type": record.record_type,
            "content": record.content,
            "ttl": record.ttl,
            "prio": record.prio,
    });
    tracing::debug!("edit body: {:#?}", body);

    let resp = client.post(url).json(&body).send().await?;
    validate_json(resp.json::<Value>().await?)?;
    Ok(())
}

pub async fn create_or_edit(client: &Client, params: &Params) -> Result<String> {
    let mut records = retrieve_by_name_type(client, params).await?;
    if records.is_empty() {
        create(client, params).await
    } else {
        let record = records.remove(0);
        let id = record.id.clone();
        if record.is_modified(params) {
            edit(client, record.modify(params)?).await?;
        }
        Ok(id)
    }
}

fn validate_json(v: Value) -> Result<Value> {
    if v["status"] != "SUCCESS" {
        let message = v["message"].as_str().unwrap_or_default();
        tracing::debug!("porkbun retreive failed: {:?}", v);
        return Err(eyre!("porkbun retreive failed: {message}"));
    }

    Ok(v)
}
