use anyhow::Result;
use chrono::{Timelike, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct AlvandConfig {
    pub cookie: String,
    /// The 'nt' token from userInfo (obtained after login)
    pub nt: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<AlvandOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://arzeshafarin.exirbroker.com/api/v1/order".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

/// Calculate the X-App-N header value dynamically
///
/// Algorithm based on the JavaScript implementation:
/// 1. Extract substring from nt (starting at position 2)
/// 2. Calculate sum of character codes from URL path
/// 3. Calculate seconds since midnight UTC (minus 2 seconds for clock skew)
/// 4. Generate header: "firstPart.secondPart"
///    - firstPart = floor(extractedValue * utcSeconds * urlCharSum)
///    - secondPart = utcSeconds * urlCharSum
pub fn calculate_x_app_n(nt: &str, url: &str) -> String {
    // Get current UTC time minus 2 seconds (for clock skew)
    let now = Utc::now() - chrono::Duration::seconds(2);

    // Calculate seconds since midnight UTC
    let utc_seconds: i64 = (3600 * now.hour() + 60 * now.minute() + now.second()) as i64;

    // Calculate sum of character codes from URL
    let url_char_sum: i64 = url.chars().map(|c| c as i64).sum();

    // Extract substring from nt starting at position 2
    let l = if nt.len() > 2 { &nt[2..] } else { nt };

    // Get the first 2 characters as the offset value
    let offset_str = if nt.len() >= 2 { &nt[0..2] } else { "0" };
    let offset: i64 = offset_str.parse().unwrap_or(0);

    // Calculate the position to extract 5 characters from
    let l_len = l.len() as i64;
    let pos = if l_len > 5 {
        (utc_seconds % (l_len - 5) - offset).abs() as usize
    } else {
        0
    };

    // Extract 5 characters from position
    let end_pos = (pos + 5).min(l.len());
    let extracted_str = if pos < l.len() { &l[pos..end_pos] } else { "0" };

    // Parse extracted value as float
    let extracted_value: f64 = extracted_str.parse().unwrap_or(0.0);

    // Calculate both parts
    let second_part = utc_seconds * url_char_sum;
    let first_part = (extracted_value * second_part as f64).floor() as i64;

    format!("{}.{}", first_part, second_part)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AlvandOrderData {
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

pub async fn send_order(config: &AlvandConfig, order: &AlvandOrderData, test_mode: bool) -> Result<()> {
    let client = reqwest::Client::new();

    // Calculate X-App-N dynamically for each request
    let x_app_n = calculate_x_app_n(&config.nt, &config.order_url);
    println!("[Alvand] Generated X-App-N: {}", x_app_n);

    let order_json = serde_json::to_string(order)?;

    // Print curl command in test mode
    if test_mode {
        println!("[Alvand] Equivalent curl command:");
        println!(r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Referer: https://arzeshafarin.exirbroker.com/exir/mainNew' \
  -H 'Content-Type: application/json' \
  -H 'X-App-N: {}' \
  -H 'Origin: https://arzeshafarin.exirbroker.com' \
  -H 'Connection: keep-alive' \
  -H 'Cookie: {}' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-origin' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  --data-raw '{}'"#,
            config.order_url, config.user_agent, x_app_n, config.cookie, order_json);
        println!();
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br, zstd"));
    headers.insert(REFERER, HeaderValue::from_static("https://arzeshafarin.exirbroker.com/exir/mainNew"));
    headers.insert("X-App-N", HeaderValue::from_str(&x_app_n)?);
    headers.insert(ORIGIN, HeaderValue::from_static("https://arzeshafarin.exirbroker.com"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert(COOKIE, HeaderValue::from_str(&config.cookie)?);
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("[Alvand] Sending order JSON: {}", order_json);

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

    println!("[Alvand] Order response status: {}", status);
    println!("[Alvand] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

