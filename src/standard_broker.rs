use crate::calibration::{self, CalibrationConfig};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, ORIGIN,
    REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36".to_string()
}

pub fn default_batch_delay() -> u64 {
    100
}

#[derive(Debug, Deserialize, Clone)]
pub struct StandardBrokersConfig {
    pub brokers: Vec<StandardBrokerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StandardBrokerConfig {
    pub name: String,
    pub cookie: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    pub order_url: String,
    pub origin: String,
    pub referer: String,
    pub orders: Vec<StandardOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<CalibrationConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StandardOrderData {
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

pub fn load_config(path: &str) -> Result<StandardBrokersConfig> {
    let config_str =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path))?;
    let config: StandardBrokersConfig =
        serde_json::from_str(&config_str).with_context(|| format!("Failed to parse {}", path))?;
    Ok(config)
}

pub fn find_broker<'a>(
    config: &'a StandardBrokersConfig,
    name: &str,
) -> Option<&'a StandardBrokerConfig> {
    config
        .brokers
        .iter()
        .find(|broker| broker.name.eq_ignore_ascii_case(name))
}

pub async fn send_order(
    broker: &StandardBrokerConfig,
    order_json: &str,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    if test_mode {
        println!("[{}] Equivalent curl command:", broker.name);
        println!(
            r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Content-Type: application/json' \
  -H 'X-Requested-With: XMLHttpRequest' \
  -H 'Origin: {}' \
  -H 'Connection: keep-alive' \
  -H 'Referer: {}' \
  -H 'Cookie: {}' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-site' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  --data-raw '{}'"#,
            broker.order_url,
            broker.user_agent,
            broker.origin,
            broker.referer,
            broker.cookie,
            order_json
        );
        println!();

        if curl_only {
            return Ok(());
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&broker.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.5"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers.insert(
        "X-Requested-With",
        HeaderValue::from_static("XMLHttpRequest"),
    );
    headers.insert(ORIGIN, HeaderValue::from_str(&broker.origin)?);
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(REFERER, HeaderValue::from_str(&broker.referer)?);
    headers.insert(COOKIE, HeaderValue::from_str(&broker.cookie)?);
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
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&body_bytes.len().to_string())?,
    );

    println!("[{}] Sending order JSON: {}", broker.name, order_json);

    let response = client
        .post(&broker.order_url)
        .headers(headers)
        .body(order_json.to_string())
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;

    let decoded_text = if response_text.contains("\\u") {
        crate::decode_unicode_escapes(&response_text)
    } else {
        response_text.clone()
    };

    println!("[{}] Order response status: {}", broker.name, status);
    println!("[{}] Order response body: {}", broker.name, decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

pub async fn run_calibration(
    broker: &StandardBrokerConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Result<calibration::CalibrationSummary> {
    let calibration = broker
        .calibration
        .as_ref()
        .context("Calibration config missing")?;

    let prefix = format!("[{}]", broker.name);
    calibration::run_calibration(&prefix, calibration, rate_limiter, || {
        send_probe(broker, client)
    })
    .await
}

async fn send_probe(
    broker: &StandardBrokerConfig,
    client: &reqwest::Client,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&broker.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(COOKIE, HeaderValue::from_str(&broker.cookie)?);

    let base_url = calibration::probe_url(&broker.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}
