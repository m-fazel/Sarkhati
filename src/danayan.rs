use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct DanayanConfig {
    pub cookie: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<DanayanOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://otapi.danayan.broker/api/v1/TseOms/RegisterOrder".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

fn default_rate_limit_ms() -> u64 {
    300
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DanayanOrderData {
    #[serde(rename = "orderValidityType")]
    pub order_validity_type: i32,
    #[serde(rename = "orderPaymentGateway")]
    pub order_payment_gateway: i32,
    pub price: i64,
    pub quantity: i64,
    #[serde(rename = "disclosedQuantity")]
    pub disclosed_quantity: Option<i64>,
    pub isin: String,
    #[serde(rename = "orderSide")]
    pub order_side: i32,
}

pub async fn send_order(config: &DanayanConfig, order: &DanayanOrderData, test_mode: bool, curl_only: bool) -> Result<()> {
    let client = reqwest::Client::new();

    let order_json = serde_json::to_string(order)?;

    // Print curl command in test mode
    if test_mode {
        println!("[Danayan] Equivalent curl command:");
        println!(r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Content-Type: application/json' \
  -H 'Origin: https://trader.danayan.broker' \
  -H 'Connection: keep-alive' \
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
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://trader.danayan.broker"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("[Danayan] Sending order JSON: {}", order_json);

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

    println!("[Danayan] Order response status: {}", status);
    println!("[Danayan] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}
