use crate::calibration::{self, CalibrationConfig};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN,
    REFERER, USER_AGENT,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Deserialize, Clone)]
pub struct SamanConfig {
    pub cookie: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<SamanOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<CalibrationConfig>,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://api.samanbourse.ir/Web/V1/Order/Post".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SamanOrderData {
    #[serde(rename = "IsSymbolCautionAgreement")]
    pub is_symbol_caution_agreement: bool,
    #[serde(rename = "CautionAgreementSelected")]
    pub caution_agreement_selected: bool,
    #[serde(rename = "IsSymbolSepahAgreement")]
    pub is_symbol_sepah_agreement: bool,
    #[serde(rename = "SepahAgreementSelected")]
    pub sepah_agreement_selected: bool,
    #[serde(rename = "orderCount")]
    pub order_count: i64,
    #[serde(rename = "orderPrice")]
    pub order_price: i64,
    #[serde(rename = "FinancialProviderId")]
    pub financial_provider_id: i32,
    #[serde(rename = "minimumQuantity")]
    pub minimum_quantity: i64,
    #[serde(rename = "maxShow")]
    pub max_show: i64,
    #[serde(rename = "orderId")]
    pub order_id: i64,
    pub isin: String,
    #[serde(rename = "orderSide")]
    pub order_side: i32,
    #[serde(rename = "orderValidity")]
    pub order_validity: i32,
    #[serde(rename = "orderValiditydate")]
    pub order_validity_date: Option<String>,
    #[serde(rename = "shortSellIsEnabled")]
    pub short_sell_is_enabled: bool,
    #[serde(rename = "shortSellIncentivePercent")]
    pub short_sell_incentive_percent: i32,
}

pub async fn send_order(
    config: &SamanConfig,
    order: &SamanOrderData,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let order_json = serde_json::to_string(order)?;

    // Print curl command in test mode
    if test_mode {
        println!("[Saman] Equivalent curl command:");
        println!(r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Content-Type: application/json' \
  -H 'X-Requested-With: XMLHttpRequest' \
  -H 'Origin: https://online.oibourse.ir' \
  -H 'Connection: keep-alive' \
  -H 'Referer: https://online.oibourse.ir/' \
  -H 'Cookie: {}' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-site' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  --data-raw '{}'"#,
            config.order_url, config.user_agent, config.cookie, order_json);
        println!();

        // If curl_only, don't send the request
        if curl_only {
            return Ok(());
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert("X-Requested-With", HeaderValue::from_static("XMLHttpRequest"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://online.oibourse.ir"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(REFERER, HeaderValue::from_static("https://online.oibourse.ir/"));
    headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    if let Some(limiter) = rate_limiter {
        limiter.wait().await;
    }

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("[Saman] Sending order JSON: {}", order_json);

    let response = client.post(&config.order_url)
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

    println!("[Saman] Order response status: {}", status);
    println!("[Saman] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

pub async fn run_calibration(
    config: &SamanConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Result<calibration::CalibrationSummary> {
    let calibration = config
        .calibration
        .as_ref()
        .context("Calibration config missing")?;

    calibration::run_calibration("[Saman]", calibration, rate_limiter, || {
        send_probe(client, config)
    })
    .await
}

async fn send_probe(
    client: &reqwest::Client,
    config: &SamanConfig,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);

    let base_url = calibration::probe_url(&config.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}
