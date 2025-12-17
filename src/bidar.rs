use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};

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
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
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

pub async fn send_order(config: &BidarConfig, order: &BidarOrderData) -> Result<()> {
    let client = reqwest::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
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

    // Authorization header
    let auth_value = if config.authorization.starts_with("Bearer ") {
        config.authorization.clone()
    } else {
        format!("Bearer {}", config.authorization)
    };
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);

    // x-user-trace header (optional)
    if !config.x_user_trace.is_empty() {
        headers.insert("x-user-trace", HeaderValue::from_str(&config.x_user_trace)?);
    }

    let order_json = serde_json::to_string(order)?;
    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("Sending order JSON: {}", order_json);

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

    println!("Order response status: {}", status);
    println!("Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

