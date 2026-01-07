use crate::calibration::{self, CalibrationConfig};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use chrono::{Timelike, Utc};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, ORIGIN,
    REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

fn default_batch_repeat() -> usize {
    1
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExirBrokersConfig {
    pub brokers: Vec<ExirBrokerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExirBrokerConfig {
    pub name: String,
    pub cookie: String,
    pub nt: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    pub order_url: String,
    pub origin: String,
    pub referer: String,
    pub orders: Vec<ExirOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default = "default_batch_repeat")]
    pub batch_repeat: usize,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<CalibrationConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExirOrderData {
    #[serde(rename = "insMaxLcode")]
    pub ins_max_lcode: String,
    #[serde(rename = "bankAccountId")]
    pub bank_account_id: i64,
    pub side: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    pub quantity: i64,
    pub price: i64,
    #[serde(rename = "validityType")]
    pub validity_type: String,
    #[serde(rename = "validityDate")]
    pub validity_date: String,
    #[serde(rename = "coreType")]
    pub core_type: String,
    #[serde(rename = "hasUnderCautionAgreement")]
    pub has_under_caution_agreement: bool,
    #[serde(rename = "dividedOrder")]
    pub divided_order: bool,
}

pub fn load_config(path: &str) -> Result<ExirBrokersConfig> {
    let config_str =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path))?;
    let config: ExirBrokersConfig =
        serde_json::from_str(&config_str).with_context(|| format!("Failed to parse {}", path))?;
    Ok(config)
}

pub fn find_broker<'a>(config: &'a ExirBrokersConfig, name: &str) -> Option<&'a ExirBrokerConfig> {
    config
        .brokers
        .iter()
        .find(|broker| broker.name.eq_ignore_ascii_case(name))
}

pub fn calculate_x_app_n(nt: &str, url: &str) -> String {
    let now = Utc::now() - chrono::Duration::seconds(2);
    let utc_seconds: i64 = (3600 * now.hour() + 60 * now.minute() + now.second()) as i64;

    let url_path = if let Some(pos) = url.find("://") {
        if let Some(path_start) = url[pos + 3..].find('/') {
            &url[pos + 3 + path_start..]
        } else {
            "/"
        }
    } else {
        url
    };

    let url_char_sum: i64 = url_path.chars().map(|c| c as i64).sum();

    let l = if nt.len() > 2 { &nt[2..] } else { nt };

    let offset_str = if nt.len() >= 2 { &nt[0..2] } else { "0" };
    let offset: i64 = offset_str.parse().unwrap_or(0);

    let l_len = l.len() as i64;
    let pos = if l_len > 5 {
        (utc_seconds % (l_len - 5) - offset).abs() as usize
    } else {
        0
    };

    let end_pos = (pos + 5).min(l.len());
    let extracted_str = if pos < l.len() { &l[pos..end_pos] } else { "0" };

    let extracted_value: f64 = extracted_str.parse().unwrap_or(0.0);

    let second_part = utc_seconds * url_char_sum;
    let first_part = (extracted_value.floor() * second_part as f64).floor() as i64;

    format!("{}.{}", first_part, second_part)
}

pub async fn send_order(
    broker: &ExirBrokerConfig,
    order_json: &str,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let x_app_n = calculate_x_app_n(&broker.nt, &broker.order_url);
    println!("[{}] Generated X-App-N: {}", broker.name, x_app_n);

    if test_mode {
        println!("[{}] Equivalent curl command:", broker.name);
        println!(
            r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Referer: {}' \
  -H 'Content-Type: application/json' \
  -H 'X-App-N: {}' \
  -H 'Origin: {}' \
  -H 'Connection: keep-alive' \
  -H 'Cookie: {}' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-origin' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  --data-raw '{}'"#,
            broker.order_url,
            broker.user_agent,
            broker.referer,
            x_app_n,
            broker.origin,
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
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.5"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers.insert(REFERER, HeaderValue::from_str(&broker.referer)?);
    headers.insert("X-App-N", HeaderValue::from_str(&x_app_n)?);
    headers.insert(ORIGIN, HeaderValue::from_str(&broker.origin)?);
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(COOKIE, HeaderValue::from_str(&broker.cookie)?);
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
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
    broker: &ExirBrokerConfig,
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
    broker: &ExirBrokerConfig,
    client: &reqwest::Client,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&broker.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(COOKIE, HeaderValue::from_str(&broker.cookie)?);
    headers.insert("nt", HeaderValue::from_str(&broker.nt)?);

    let base_url = calibration::probe_url(&broker.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}
