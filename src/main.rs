use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    cookie: String,
    #[serde(default)]
    authorization: String,
    #[serde(default = "default_user_agent")]
    user_agent: String,
    orders: Vec<OrderData>,
    #[serde(default = "default_batch_delay")]
    batch_delay_ms: u64,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_batch_delay() -> u64 {
    100  // Default 100ms between batches
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OrderData {
    #[serde(rename = "orderSide")]
    order_side: String,
    price: i32,
    quantity: i32,
    #[serde(rename = "symbolIsin")]
    symbol_isin: String,
    #[serde(rename = "validityType")]
    validity_type: i32,
    #[serde(rename = "validityDate")]
    validity_date: Option<String>,
    #[serde(rename = "orderFrom")]
    order_from: String,
}

const ORDER_URL: &str = "https://mofidonline.com/apigateway/api/v1/Order/send";

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config_str = fs::read_to_string("config.json")
        .context("Failed to read config.json")?;
    let config: Config = serde_json::from_str(&config_str)
        .context("Failed to parse config.json")?;

    println!("Starting Sarkhati - Order Sender");

    // Determine authentication method
    let use_cookie = !config.cookie.is_empty() && config.cookie != "PASTE_YOUR_COOKIE_HERE";
    let use_auth = !config.authorization.is_empty();

    if use_cookie {
        println!("Using Cookie authentication");
        println!("Cookie preview: {}...", &config.cookie[..config.cookie.len().min(50)]);
    } else if use_auth {
        println!("Using Authorization header");
        println!("Authorization preview: Bearer {}...", &config.authorization[..config.authorization.len().min(30)]);
    } else {
        anyhow::bail!("No authentication method configured. Please set either 'cookie' or 'authorization' in config.json");
    }

    // Validate that we have at least one order
    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config.json. Please add at least one order to the 'orders' array.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending (non-blocking mode)...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders in parallel ===", batch_number, config.orders.len());

        // Create tasks for all orders
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = Config {
                cookie: config.cookie.clone(),
                authorization: config.authorization.clone(),
                user_agent: config.user_agent.clone(),
                orders: vec![],  // Not needed in the clone
                batch_delay_ms: config.batch_delay_ms,
            };
            let order_clone = order.clone();
            let batch = batch_number;

            // Spawn each order as a separate task that handles its own result
            tokio::spawn(async move {
                match send_order(&config_clone, &order_clone).await {
                    Ok(_) => {
                        println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1);
                    }
                    Err(e) => {
                        eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e);
                    }
                }
            });
        }

        // Delay before sending next batch (configured in config.json)
        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

async fn send_order(config: &Config, order: &OrderData) -> Result<()> {
    // Build client - reqwest automatically handles decompression
    let client = reqwest::Client::new();

    // Build headers
    // Note: Don't manually set Accept-Encoding - let reqwest handle it automatically
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.5"));
    headers.insert(REFERER, HeaderValue::from_static("https://tg.mofidonline.com/"));

    // Add authentication - prefer cookie if available, otherwise use authorization
    let use_cookie = !config.cookie.is_empty() && config.cookie != "PASTE_YOUR_COOKIE_HERE";

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

    // Serialize order data
    let order_json = serde_json::to_string(order)?;
    let body_bytes = order_json.as_bytes();

    // Set Content-Type and Content-Length headers
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&body_bytes.len().to_string())?);

    println!("Sending order JSON: {}", order_json);

    // Send POST request with body
    let response = client.post(ORDER_URL)
        .headers(headers)
        .body(order_json)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;

    // Decode Unicode escape sequences only if they exist in the response
    let decoded_text = if response_text.contains("\\u") {
        decode_unicode_escapes(&response_text)
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

/// Decode Unicode escape sequences (e.g., \u0645) to actual characters
fn decode_unicode_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next_ch) = chars.peek() {
                if next_ch == 'u' {
                    chars.next(); // consume 'u'

                    // Collect the next 4 hex digits
                    let hex_digits: String = chars.by_ref().take(4).collect();

                    if hex_digits.len() == 4 {
                        if let Ok(code_point) = u32::from_str_radix(&hex_digits, 16) {
                            if let Some(unicode_char) = char::from_u32(code_point) {
                                result.push(unicode_char);
                                continue;
                            }
                        }
                    }

                    // If parsing failed, keep the original sequence
                    result.push('\\');
                    result.push('u');
                    result.push_str(&hex_digits);
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}
