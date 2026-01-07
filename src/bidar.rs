use crate::calibration::{self, CalibrationConfig};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue,
    ORIGIN, REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Deserialize, Clone)]
pub struct BidarConfig {
    pub authorization: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    #[serde(default)]
    pub x_user_trace: String,
    pub orders: Vec<BidarOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<CalibrationConfig>,
    #[serde(default)]
    pub delay_model: BidarDelayModel,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36".to_string()
}

fn default_order_url() -> String {
    "https://api.bidartrader.ir/trader/v1/order/buy".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BidarOrderData {
    #[serde(rename = "type")]
    pub order_type: String,
    pub quantity: String,
    pub isin: String,
    pub validity: String,
    pub price: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BidarDelayModel {
    Rtt,
    HalfRtt,
}

impl Default for BidarDelayModel {
    fn default() -> Self {
        Self::Rtt
    }
}

pub async fn send_order(
    config: &BidarConfig,
    order: &BidarOrderData,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let order_json = serde_json::to_string(order)?;

    // Authorization header
    let auth_value = if config.authorization.starts_with("Bearer ") {
        config.authorization.clone()
    } else {
        format!("Bearer {}", config.authorization)
    };

    // Print curl command in test mode
    if test_mode {
        let x_user_trace_header = if !config.x_user_trace.is_empty() {
            format!("-H 'x-user-trace: {}' \\\n  ", config.x_user_trace)
        } else {
            String::new()
        };
        println!("[Bidar] Equivalent curl command:");
        println!(
            r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Content-Type: application/json' \
  -H 'Referer: https://bidartrader.ir/' \
  -H 'Origin: https://bidartrader.ir' \
  -H 'Authorization: {}' \
  {}-H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-site' \
  -H 'Connection: keep-alive' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  -H 'TE: trailers' \
  --data-raw '{}'"#,
            config.order_url, config.user_agent, auth_value, x_user_trace_header, order_json
        );
        println!();

        // If curl_only, don't send the request
        if curl_only {
            return Ok(());
        }
    }

    if let Some(limiter) = rate_limiter {
        limiter.wait().await;
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.5"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://bidartrader.ir/"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://bidartrader.ir"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("TE", HeaderValue::from_static("trailers"));

    headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);

    // x-user-trace header (optional)
    if !config.x_user_trace.is_empty() {
        headers.insert("x-user-trace", HeaderValue::from_str(&config.x_user_trace)?);
    }

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&body_bytes.len().to_string())?,
    );

    println!("[Bidar] Sending order JSON: {}", order_json);

    let response = client
        .post(&config.order_url)
        .headers(headers)
        .body(order_json)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;

    let decoded_text = if response_text.contains("\\u") {
        crate::decode_unicode_escapes(&response_text)
    } else {
        response_text.clone()
    };

    println!("[Bidar] Order response status: {}", status);
    println!("[Bidar] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

pub async fn run_calibration(
    config: &BidarConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Result<calibration::CalibrationSummary> {
    let calibration = config
        .calibration
        .as_ref()
        .context("Calibration config missing")?;

    calibration::run_calibration("[Bidar]", calibration, rate_limiter, || {
        send_probe(client, config)
    })
    .await
}

async fn send_probe(
    client: &reqwest::Client,
    config: &BidarConfig,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));

    if !config.authorization.is_empty() {
        let auth_value = if config.authorization.starts_with("Bearer ") {
            config.authorization.clone()
        } else {
            format!("Bearer {}", config.authorization)
        };
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);
    }

    let base_url = calibration::probe_url(&config.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}
