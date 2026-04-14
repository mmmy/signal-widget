use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::config::ApiConfig;
use crate::domain::SignalKey;

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(cfg: &ApiConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key: cfg.api_key.clone(),
        }
    }

    pub async fn fetch_signals(&self, body: &FetchSignalsRequest) -> Result<SignalPage> {
        let url = format!("{}/api/open/watch-list/symbol-signals", self.base_url);
        let response = self
            .http
            .post(url)
            .header("x-api-key", &self.api_key)
            .json(body)
            .send()
            .await
            .context("request symbol-signals failed")?;
        decode_json(response).await
    }

    pub async fn mark_read(&self, key: &SignalKey, read: bool) -> Result<bool> {
        let url = format!("{}/api/open/watch-list/symbol-alert/read-status", self.base_url);
        let body = ReadStatusRequest {
            symbol: key.symbol.clone(),
            period: key.period.clone(),
            signal_type: key.signal_type.clone(),
            read,
        };
        let response = self
            .http
            .post(url)
            .header("x-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .context("request read-status failed")?;
        decode_json(response).await
    }
}

async fn decode_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        bail!("api unauthorized: invalid x-api-key");
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("api error status {} body {}", status, body);
    }
    response.json::<T>().await.context("failed to decode response json")
}

#[derive(Debug, Clone, Serialize)]
pub struct FetchSignalsRequest {
    pub symbols: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periods: Option<String>,
    #[serde(rename = "signalTypes", skip_serializing_if = "Option::is_none")]
    pub signal_types: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(rename = "pageSize", skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalPage {
    pub total: u64,
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
    #[serde(default)]
    pub data: Vec<SignalRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalRow {
    pub symbol: String,
    pub period: String,
    #[allow(dead_code)]
    pub t: i64,
    #[serde(default)]
    pub signals: HashMap<String, SignalState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalState {
    pub sd: i32,
    pub t: i64,
    #[serde(default)]
    pub read: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ReadStatusRequest {
    symbol: String,
    period: String,
    #[serde(rename = "signalType")]
    signal_type: String,
    read: bool,
}
