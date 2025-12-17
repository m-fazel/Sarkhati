use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct MofidConfig {
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub authorization: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<MofidOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://mofidonline.com/apigateway/api/v1/Order/send".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MofidOrderData {
    #[serde(rename = "orderSide")]
    pub order_side: String,
    pub price: i64,
    pub quantity: i64,
    #[serde(rename = "symbolIsin")]
    pub symbol_isin: String,
    #[serde(rename = "validityType")]
    pub validity_type: i32,
    #[serde(rename = "validityDate")]
    pub validity_date: Option<String>,
    #[serde(rename = "orderFrom")]
    pub order_from: String,
}

pub async fn send_order(config: &MofidConfig, order: &MofidOrderData, test_mode: bool) -> Result<()> {
    let client = reqwest::Client::new();

    let use_cookie = !config.cookie.is_empty() && config.cookie != "PASTE_YOUR_COOKIE_HERE";
    let order_json = serde_json::to_string(order)?;

    // Print curl command in test mode
    if test_mode {
        let auth_header = if use_cookie {
            format!("-H 'Cookie: {}'", config.cookie)
        } else {
            format!("-H 'Authorization: Bearer {}'", config.authorization)
        };
        println!("[Mofid] Equivalent curl command:");
        println!(r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Referer: https://tg.mofidonline.com/' \
  -H 'Content-Type: application/json' \
  -H 'x-appname: titan' \
  {} \
  -H 'Origin: https://tg.mofidonline.com' \
  -H 'Connection: keep-alive' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-site' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  --data-raw '{}'"#,
            config.order_url, config.user_agent, auth_header, order_json);
        println!();
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert(REFERER, HeaderValue::from_static("https://tg.mofidonline.com/"));

    if use_cookie {
        headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);
    } else if !config.authorization.is_empty() {
        let auth_value = format!("Bearer {}", config.authorization);
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);
    }

    headers.insert("x-appname", HeaderValue::from_static("titan"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://tg.mofidonline.com"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("[Mofid] Sending order JSON: {}", order_json);

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

    println!("[Mofid] Order response status: {}", status);
    println!("[Mofid] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

