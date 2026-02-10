use crate::calibration::{self, CalibrationConfig};
use crate::logging::{log_info, log_raw_stdout};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36".to_string()
}

fn default_order_url() -> String {
    "https://api-mts.orbis.easytrader.ir/core/api/v2/order".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

fn default_batch_repeat() -> usize {
    1
}

#[derive(Debug, Deserialize, Clone)]
pub struct EasyTraderBrokersConfig {
    pub accounts: Vec<EasyTraderBrokerConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum EasyTraderBrokersConfigFile {
    Single(EasyTraderBrokerConfig),
    Multiple {
        accounts: Vec<EasyTraderBrokerConfig>,
    },
    List(Vec<EasyTraderBrokerConfig>),
}

#[derive(Debug, Deserialize, Clone)]
pub struct EasyTraderBrokerConfig {
    pub name: String,
    pub authorization: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<EasyTraderOrderData>,
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
pub struct EasyTraderOrderData {
    pub price: i64,
    pub quantity: i64,
    pub side: i32,
    #[serde(rename = "validityType")]
    pub validity_type: i32,
    #[serde(rename = "symbolIsin")]
    pub symbol_isin: String,
    #[serde(rename = "orderModelType")]
    pub order_model_type: i32,
    #[serde(rename = "orderFrom")]
    pub order_from: i32,
}

#[derive(Debug, Serialize)]
struct EasyTraderOrderPayload<'a> {
    order: &'a EasyTraderOrderData,
}

pub fn load_config(path: &str) -> Result<EasyTraderBrokersConfig> {
    let config_str =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path))?;
    let config_file: EasyTraderBrokersConfigFile =
        serde_json::from_str(&config_str).with_context(|| format!("Failed to parse {}", path))?;
    let accounts = match config_file {
        EasyTraderBrokersConfigFile::Single(config) => vec![config],
        EasyTraderBrokersConfigFile::Multiple { accounts } => accounts,
        EasyTraderBrokersConfigFile::List(accounts) => accounts,
    };
    Ok(EasyTraderBrokersConfig { accounts })
}

pub fn find_broker<'a>(
    config: &'a EasyTraderBrokersConfig,
    name: &str,
) -> Option<&'a EasyTraderBrokerConfig> {
    config
        .accounts
        .iter()
        .find(|broker| broker.name.eq_ignore_ascii_case(name))
}

pub async fn send_order(
    broker: &EasyTraderBrokerConfig,
    order: &EasyTraderOrderData,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let payload = EasyTraderOrderPayload { order };
    let payload_json = serde_json::to_string(&payload)?;
    let token = broker
        .authorization
        .strip_prefix("Bearer ")
        .unwrap_or(&broker.authorization);
    let authorization = format!("Bearer {}", token);

    if test_mode {
        log_info(&broker.name, "Equivalent curl command:");
        log_raw_stdout(&format!(
            r#"curl --location '{}' \
  --header 'Authorization: {}' \
  --header 'Content-Type: application/json' \
  --header 'User-Agent: {}' \
  --data '{}'"#,
            broker.order_url, authorization, broker.user_agent, payload_json
        ));
        log_raw_stdout("");

        if curl_only {
            return Ok(());
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&broker.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&authorization)?);

    if let Some(limiter) = rate_limiter {
        limiter.wait().await;
    }

    let body_bytes = payload_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&body_bytes.len().to_string())?,
    );

    let response = client
        .post(&broker.order_url)
        .headers(headers)
        .body(payload_json)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;

    let decoded_text = if response_text.contains("\\u") {
        crate::decode_unicode_escapes(&response_text)
    } else {
        response_text.clone()
    };

    log_info(&broker.name, &format!("Order response status: {}", status));

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

pub async fn run_calibration(
    broker: &EasyTraderBrokerConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
    deadline_epoch_ms: Option<i64>,
) -> Result<calibration::CalibrationSummary> {
    let calibration = broker
        .calibration
        .as_ref()
        .context("Calibration config missing")?;

    let prefix = format!("[{}]", broker.name);
    calibration::run_calibration(
        &prefix,
        calibration,
        rate_limiter,
        deadline_epoch_ms,
        || send_probe(broker, client),
    )
    .await
}

async fn send_probe(
    broker: &EasyTraderBrokerConfig,
    client: &reqwest::Client,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let token = broker
        .authorization
        .strip_prefix("Bearer ")
        .unwrap_or(&broker.authorization);
    let authorization = format!("Bearer {}", token);

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&broker.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&authorization)?);

    let base_url = calibration::probe_url(&broker.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}
