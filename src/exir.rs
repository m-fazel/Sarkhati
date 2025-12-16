use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct ExirConfig {
    pub cookie: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    #[serde(default = "default_x_app_n")]
    pub x_app_n: String,
    pub orders: Vec<ExirOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://arzeshafarin.exirbroker.com/api/v1/order".to_string()
}

fn default_x_app_n() -> String {
    "1824632377792.35566496".to_string()
}

fn default_batch_delay() -> u64 {
    100
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

pub async fn send_order(config: &ExirConfig, order: &ExirOrderData) -> Result<()> {
    let client = reqwest::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert(REFERER, HeaderValue::from_static("https://arzeshafarin.exirbroker.com/exir/mainNew"));
    headers.insert("X-App-N", HeaderValue::from_str(&config.x_app_n)?);
    headers.insert(ORIGIN, HeaderValue::from_static("https://arzeshafarin.exirbroker.com"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

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

