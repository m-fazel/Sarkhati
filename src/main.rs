use anyhow::{Context, Result};
use chrono::TimeZone;
use std::env;
use std::fs;

mod alvand;
mod bidar;
mod bmi;
mod danayan;
mod mofid;
mod ordibehesht;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Broker {
    Mofid,
    Bmi,
    Danayan,
    Ordibehesht,
    Alvand,
    Bidar,
    All,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // Check for test flag
    let test_mode = args.iter().any(|a| a == "test" || a == "--test");
    // Check for curl flag (only print curl command, don't send request)
    let curl_only = args.iter().any(|a| a == "curl" || a == "--curl");

    let broker = match args.get(1).map(|s| s.as_str()) {
        Some("mofid") => Broker::Mofid,
        Some("bmi") => Broker::Bmi,
        Some("danayan") => Broker::Danayan,
        Some("ordibehesht") => Broker::Ordibehesht,
        Some("alvand") => Broker::Alvand,
        Some("bidar") => Broker::Bidar,
        Some("all") => Broker::All,
        Some("test") | Some("--test") | Some("curl") | Some("--curl") => {
            eprintln!(
                "Usage: {} <mofid|bmi|danayan|ordibehesht|alvand|bidar|all> [test] [curl]",
                args[0]
            );
            eprintln!("The 'test' and 'curl' flags should come after the broker name.");
            std::process::exit(1);
        }
        Some(other) => {
            eprintln!("Unknown broker: {}", other);
            eprintln!(
                "Usage: {} <mofid|bmi|danayan|ordibehesht|alvand|bidar|all> [test] [curl]",
                args[0]
            );
            std::process::exit(1);
        }
        None => {
            eprintln!(
                "Usage: {} <mofid|bmi|danayan|ordibehesht|alvand|bidar|all> [test] [curl]",
                args[0]
            );
            std::process::exit(1);
        }
    };

    if test_mode {
        if curl_only {
            println!(
                "*** TEST MODE + CURL ONLY: Will print curl commands without sending requests ***\n"
            );
        } else {
            println!("*** TEST MODE: Loop will run only once ***\n");
        }
    }

    match broker {
        Broker::Mofid => run_mofid(test_mode, curl_only).await,
        Broker::Bmi => run_bmi(test_mode, curl_only).await,
        Broker::Danayan => run_danayan(test_mode, curl_only).await,
        Broker::Ordibehesht => run_ordibehesht(test_mode, curl_only).await,
        Broker::Alvand => run_alvand(test_mode, curl_only).await,
        Broker::Bidar => run_bidar(test_mode, curl_only).await,
        Broker::All => run_all(test_mode, curl_only).await,
    }
}

async fn run_all(test_mode: bool, curl_only: bool) -> Result<()> {
    println!("Starting Sarkhati - All Brokers in Parallel\n");

    let alvand_handle = tokio::spawn(async move {
        if let Err(e) = run_alvand(test_mode, curl_only).await {
            eprintln!("[Alvand] Error: {}", e);
        }
    });

    let mofid_handle = tokio::spawn(async move {
        if let Err(e) = run_mofid(test_mode, curl_only).await {
            eprintln!("[Mofid] Error: {}", e);
        }
    });

    let bmi_handle = tokio::spawn(async move {
        if let Err(e) = run_bmi(test_mode, curl_only).await {
            eprintln!("[BMI] Error: {}", e);
        }
    });

    let danayan_handle = tokio::spawn(async move {
        if let Err(e) = run_danayan(test_mode, curl_only).await {
            eprintln!("[Danayan] Error: {}", e);
        }
    });

    let ordibehesht_handle = tokio::spawn(async move {
        if let Err(e) = run_ordibehesht(test_mode, curl_only).await {
            eprintln!("[Ordibehesht] Error: {}", e);
        }
    });

    let bidar_handle = tokio::spawn(async move {
        if let Err(e) = run_bidar(test_mode, curl_only).await {
            eprintln!("[Bidar] Error: {}", e);
        }
    });

    let _ = tokio::join!(
        alvand_handle,
        mofid_handle,
        bmi_handle,
        danayan_handle,
        ordibehesht_handle,
        bidar_handle
    );

    Ok(())
}

async fn run_mofid(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_mofid.json").context("Failed to read config_mofid.json")?;
    let config: mofid::MofidConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_mofid.json")?;

    println!("Starting Sarkhati - Mofid Online Order Sender");

    let use_cookie = !config.cookie.is_empty() && config.cookie != "PASTE_YOUR_COOKIE_HERE";
    let use_auth = !config.authorization.is_empty();

    if use_cookie {
        println!("Using Cookie authentication");
        println!(
            "Cookie preview: {}...",
            &config.cookie[..config.cookie.len().min(50)]
        );
    } else if use_auth {
        println!("Using Authorization header");
        println!(
            "Authorization preview: Bearer {}...",
            &config.authorization[..config.authorization.len().min(30)]
        );
    } else {
        anyhow::bail!(
            "No authentication method configured. Please set either 'cookie' or 'authorization' in config.json"
        );
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
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;

            let handle = tokio::spawn(async move {
                match mofid::send_order(&config_clone, &order_clone, is_test, is_curl_only).await {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            for handle in handles {
                let _ = handle.await;
            }
            println!("[Mofid] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
}

async fn run_bmi(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_bmi.json").context("Failed to read config_bmi.json")?;
    let config: bmi::BmiConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_bmi.json")?;

    println!("Starting Sarkhati - BMI Bourse Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for BMI Bourse. Please set 'cookie' in config.json");
    }

    println!("Using Cookie authentication");
    println!(
        "Cookie preview: {}...",
        &config.cookie[..config.cookie.len().min(50)]
    );

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
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;

            let handle = tokio::spawn(async move {
                match bmi::send_order(&config_clone, &order_clone, is_test, is_curl_only).await {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            for handle in handles {
                let _ = handle.await;
            }
            println!("[BMI] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
}

async fn run_danayan(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_danayan.json").context("Failed to read config_danayan.json")?;
    let config: danayan::DanayanConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_danayan.json")?;

    println!("Starting Sarkhati - Danayan Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for Danayan. Please set 'cookie' in config_danayan.json");
    }

    println!("Using Cookie authentication");
    println!(
        "Cookie preview: {}...",
        &config.cookie[..config.cookie.len().min(50)]
    );

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
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;

            let handle = tokio::spawn(async move {
                match danayan::send_order(&config_clone, &order_clone, is_test, is_curl_only).await
                {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            for handle in handles {
                let _ = handle.await;
            }
            println!("[Danayan] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
}

async fn run_ordibehesht(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str = fs::read_to_string("config_ordibehesht.json")
        .context("Failed to read config_ordibehesht.json")?;
    let config: ordibehesht::OrdibeheshtConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_ordibehesht.json")?;

    println!("Starting Sarkhati - Ordibehesht Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!(
            "Cookie is required for Ordibehesht. Please set 'cookie' in config_ordibehesht.json"
        );
    }

    println!("Using Cookie authentication");
    println!(
        "Cookie preview: {}...",
        &config.cookie[..config.cookie.len().min(50)]
    );

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_ordibehesht.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;

            let handle = tokio::spawn(async move {
                match ordibehesht::send_order(&config_clone, &order_clone, is_test, is_curl_only)
                    .await
                {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            for handle in handles {
                let _ = handle.await;
            }
            println!("[Ordibehesht] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
}

async fn run_alvand(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_alvand.json").context("Failed to read config_alvand.json")?;
    let config: alvand::AlvandConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_alvand.json")?;

    println!("Starting Sarkhati - Alvand Order Sender");

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for Alvand. Please set 'cookie' in config_alvand.json");
    }

    println!("Using Cookie authentication");
    println!(
        "Cookie preview: {}...",
        &config.cookie[..config.cookie.len().min(50)]
    );

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_alvand.json.");
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;

            let handle = tokio::spawn(async move {
                match alvand::send_order(&config_clone, &order_clone, is_test, is_curl_only).await {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            // Wait for all tasks to complete in test mode
            for handle in handles {
                let _ = handle.await;
            }
            println!("[Alvand] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
}

async fn run_bidar(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_bidar.json").context("Failed to read config_bidar.json")?;
    let config: bidar::BidarConfig =
        serde_json::from_str(&config_str).context("Failed to parse config_bidar.json")?;

    println!("Starting Sarkhati - Bidar Trader Order Sender");

    if config.authorization.is_empty() {
        anyhow::bail!(
            "Authorization token is required for Bidar. Please set 'authorization' in config_bidar.json"
        );
    }

    println!("Using Bearer token authentication");
    println!(
        "Token preview: {}...",
        &config.authorization[..config.authorization.len().min(50)]
    );

    if config.orders.is_empty() {
        anyhow::bail!("No orders configured in config_bidar.json.");
    }

    let rate_limiter = std::sync::Arc::new(bidar::RateLimiter::new(config.rate_limit_ms));

    if let Some(target_time_str) = &config.target_time {
        println!(
            "[Bidar] Scheduled mode enabled for target time {}",
            target_time_str
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let now = chrono::Local::now();
        let today = now.date_naive();
        let target_datetime = chrono::Local
            .from_local_datetime(&today.and_time(target_time))
            .single()
            .context("Failed to resolve target_time in local timezone")?;

        if target_datetime <= now {
            anyhow::bail!("target_time has already passed for today");
        }

        let client = reqwest::Client::new();
        let mut last_wall_time = std::time::SystemTime::now();

        let calibration_enabled = config
            .calibration
            .as_ref()
            .map_or(false, |calibration| calibration.enabled);

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = bidar::run_calibration(&config, &client, rate_limiter.as_ref()).await?;
            let mut estimated_delay_ms = summary.estimated_delay_ms;
            match config.delay_model {
                bidar::BidarDelayModel::Rtt => {}
                bidar::BidarDelayModel::HalfRtt => {
                    estimated_delay_ms = (estimated_delay_ms + 1) / 2;
                    println!(
                        "[Bidar] Delay model half_rtt applied, estimate now {}ms",
                        estimated_delay_ms
                    );
                }
            }
            (
                estimated_delay_ms,
                config
                    .calibration
                    .as_ref()
                    .map(|calibration| calibration.safety_margin_ms)
                    .unwrap_or_default(),
                summary.last_probe_wall_time,
            )
        } else {
            println!("[Bidar] Calibration disabled; using zero delay estimate.");
            (0, 0, std::time::SystemTime::now())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_time =
            target_datetime - chrono::Duration::milliseconds(effective_delay_ms as i64);
        let final_send_system: std::time::SystemTime = final_send_time.into();

        if final_send_time <= chrono::Local::now() {
            anyhow::bail!(
                "final_send_time has already passed; increase target_time or reduce delay"
            );
        }

        if calibration_enabled {
            if let Ok(delta) = final_send_system.duration_since(last_probe_wall_time) {
                if delta < std::time::Duration::from_millis(config.rate_limit_ms) {
                    anyhow::bail!(
                        "Last probe is too close to final_send_time; ensure at least {}ms gap",
                        config.rate_limit_ms
                    );
                }
            }
        }

        println!(
            "[Bidar] target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
            target_datetime.format("%H:%M:%S%.3f"),
            final_send_time.format("%H:%M:%S%.3f"),
            estimated_delay_ms,
            safety_margin_ms,
            effective_delay_ms
        );

        let now_wall = std::time::SystemTime::now();
        if now_wall < last_wall_time {
            anyhow::bail!("System clock moved backwards; aborting");
        }
        last_wall_time = now_wall;

        let until_final = final_send_system
            .duration_since(now_wall)
            .context("final_send_time already passed")?;

        let spin_threshold = std::time::Duration::from_millis(5);
        if until_final > spin_threshold {
            tokio::time::sleep(until_final - spin_threshold).await;
        }

        loop {
            let current = std::time::SystemTime::now();
            if current < last_wall_time {
                anyhow::bail!("System clock moved backwards; aborting");
            }
            if current >= final_send_system {
                break;
            }
            last_wall_time = current;
            std::hint::spin_loop();
        }

        let actual_send_time = chrono::Local::now();
        let drift_micros = actual_send_time
            .signed_duration_since(final_send_time)
            .num_microseconds()
            .unwrap_or_default();
        println!(
            "[Bidar] Sending final order at {} (drift {}µs)",
            actual_send_time.format("%H:%M:%S%.3f"),
            drift_micros
        );

        for (index, order) in config.orders.iter().enumerate() {
            bidar::send_order(
                &config,
                order,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            .with_context(|| format!("Failed to send scheduled order #{}", index + 1))?;
        }

        return Ok(());
    }

    println!("Loaded {} order(s) from config", config.orders.len());
    println!("Batch delay: {}ms between batches", config.batch_delay_ms);
    println!("Starting continuous order sending...\n");

    let mut batch_number = 0u64;
    let batch_delay = config.batch_delay_ms;

    loop {
        batch_number += 1;
        println!(
            "=== Batch #{}: Sending {} orders ===",
            batch_number,
            config.orders.len()
        );

        let mut handles = Vec::new();
        for (index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let batch = batch_number;
            let is_test = test_mode;
            let is_curl_only = curl_only;
            let limiter = rate_limiter.clone();

            let handle = tokio::spawn(async move {
                match bidar::send_order(
                    &config_clone,
                    &order_clone,
                    is_test,
                    is_curl_only,
                    Some(limiter.as_ref()),
                )
                .await
                {
                    Ok(_) => println!(
                        "✓ Batch #{}, Order #{}: Sent successfully",
                        batch,
                        index + 1
                    ),
                    Err(e) => eprintln!("✗ Batch #{}, Order #{}: Failed - {}", batch, index + 1, e),
                }
            });
            handles.push(handle);
        }

        if test_mode {
            for handle in handles {
                let _ = handle.await;
            }
            println!("[Bidar] Test mode: exiting after one batch");
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(batch_delay)).await;
    }

    Ok(())
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
