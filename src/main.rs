use anyhow::{Context, Result};
use std::env;
use std::fs;

mod bmi;
mod danayan;
mod exir;
mod mofid;
mod oi;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Broker {
    Mofid,
    Bmi,
    Danayan,
    Oi,
    Exir,
    All,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let broker = match args.get(1).map(|s| s.as_str()) {
        Some("mofid") => Broker::Mofid,
        Some("bmi") => Broker::Bmi,
        Some("danayan") => Broker::Danayan,
        Some("oi") => Broker::Oi,
        Some("exir") => Broker::Exir,
        Some("all") => Broker::All,
        Some(other) => {
            eprintln!("Unknown broker: {}", other);
            eprintln!("Usage: {} <mofid|bmi|danayan|oi|exir|all>", args[0]);
            std::process::exit(1);
        }
        None => {
            eprintln!("Usage: {} <mofid|bmi|danayan|oi|exir|all>", args[0]);
            std::process::exit(1);
        }
    };

    match broker {
        Broker::Mofid => run_mofid().await,
        Broker::Bmi => run_bmi().await,
        Broker::Danayan => run_danayan().await,
        Broker::Oi => run_oi().await,
        Broker::Exir => run_exir().await,
        Broker::All => run_all().await,
    }
}

async fn run_all() -> Result<()> {
    println!("Starting Sarkhati - All Brokers in Parallel\n");

    let mofid_handle = tokio::spawn(async {
        if let Err(e) = run_mofid().await {
            eprintln!("[Mofid] Error: {}", e);
        }
    });

    let bmi_handle = tokio::spawn(async {
        if let Err(e) = run_bmi().await {
            eprintln!("[BMI] Error: {}", e);
        }
    });

    let danayan_handle = tokio::spawn(async {
        if let Err(e) = run_danayan().await {
            eprintln!("[Danayan] Error: {}", e);
        }
    });

    let oi_handle = tokio::spawn(async {
        if let Err(e) = run_oi().await {
            eprintln!("[OI] Error: {}", e);
        }
    });

    let exir_handle = tokio::spawn(async {
        if let Err(e) = run_exir().await {
            eprintln!("[Exir] Error: {}", e);
        }
    });

    let _ = tokio::join!(mofid_handle, bmi_handle, danayan_handle, oi_handle, exir_handle);

    Ok(())
}

async fn run_mofid() -> Result<()> {
    let config_str = fs::read_to_string("config_mofid.json")
        .context("Failed to read config_mofid.json")?;
    let config: mofid::MofidConfig = serde_json::from_str(&config_str)
        .context("Failed to parse config_mofid.json")?;

    println!("Starting Sarkhati - Mofid Online Order Sender");

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

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders ===", batch_number, config.orders.len());

        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;

            tokio::spawn(async move {
                match mofid::send_order(&config_clone, &order_clone).await {
                    Ok(_) => println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

async fn run_bmi() -> Result<()> {
    let config_str = fs::read_to_string("config_bmi.json")
        .context("Failed to read config_bmi.json")?;
    let config: bmi::BmiConfig = serde_json::from_str(&config_str)
        .context("Failed to parse config_bmi.json")?;

    println!("Starting Sarkhati - BMI Bourse Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for BMI Bourse. Please set 'cookie' in config.json");
    }

    println!("Using Cookie authentication");
    println!("Cookie preview: {}...", &config.cookie[..config.cookie.len().min(50)]);

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders ===", batch_number, config.orders.len());

        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;

            tokio::spawn(async move {
                match bmi::send_order(&config_clone, &order_clone).await {
                    Ok(_) => println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

async fn run_danayan() -> Result<()> {
    let config_str = fs::read_to_string("config_danayan.json")
        .context("Failed to read config_danayan.json")?;
    let config: danayan::DanayanConfig = serde_json::from_str(&config_str)
        .context("Failed to parse config_danayan.json")?;

    println!("Starting Sarkhati - Danayan Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for Danayan. Please set 'cookie' in config_danayan.json");
    }

    println!("Using Cookie authentication");
    println!("Cookie preview: {}...", &config.cookie[..config.cookie.len().min(50)]);

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_danayan.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders ===", batch_number, config.orders.len());

        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;

            tokio::spawn(async move {
                match danayan::send_order(&config_clone, &order_clone).await {
                    Ok(_) => println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

async fn run_oi() -> Result<()> {
    let config_str = fs::read_to_string("config_oi.json")
        .context("Failed to read config_oi.json")?;
    let config: oi::OiConfig = serde_json::from_str(&config_str)
        .context("Failed to parse config_oi.json")?;

    println!("Starting Sarkhati - OI Bourse Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for OI Bourse. Please set 'cookie' in config_oi.json");
    }

    println!("Using Cookie authentication");
    println!("Cookie preview: {}...", &config.cookie[..config.cookie.len().min(50)]);

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_oi.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders ===", batch_number, config.orders.len());

        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;

            tokio::spawn(async move {
                match oi::send_order(&config_clone, &order_clone).await {
                    Ok(_) => println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

async fn run_exir() -> Result<()> {
    let config_str = fs::read_to_string("config_exir.json")
        .context("Failed to read config_exir.json")?;
    let config: exir::ExirConfig = serde_json::from_str(&config_str)
        .context("Failed to parse config_exir.json")?;

    println!("Starting Sarkhati - Exir Broker Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for Exir Broker. Please set 'cookie' in config_exir.json");
    }

    println!("Using Cookie authentication");
    println!("Cookie preview: {}...", &config.cookie[..config.cookie.len().min(50)]);

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_exir.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!("=== Batch #{}: Sending {} orders ===", batch_number, config.orders.len());

        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;

            tokio::spawn(async move {
                match exir::send_order(&config_clone, &order_clone).await {
                    Ok(_) => println!("✓ Batch #{}, Order #{}: Sent successfully", batch, index + 1),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }
}

/// Decode Unicode escape sequences (e.g., \u0645) to actual characters
pub fn decode_unicode_escapes(s: &str) -> String {
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
