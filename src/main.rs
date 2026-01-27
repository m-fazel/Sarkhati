use anyhow::{Context, Result};
use chrono::TimeZone;
use chrono_tz::Asia::Tehran;
use std::env;
use std::fs;

mod bidar;
mod calibration;
mod danayan;
mod exir_broker;
mod global_config;
mod logging;
mod mofid;
mod rate_limiter;
mod standard_broker;
mod time_reference;

use crate::logging::{log_error, log_info, log_success, log_warn};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // Check for test flag
    let test_mode = args.iter().any(|a| a == "test" || a == "--test");
    // Check for curl flag (only print curl command, don't send request)
    let curl_only = args.iter().any(|a| a == "curl" || a == "--curl");

    let broker = match args.get(1).map(|s| s.as_str()) {
        Some("test") | Some("--test") | Some("curl") | Some("--curl") => {
            log_error(
                "CLI",
                &format!(
                    "Usage: {} <mofid|danayan|bidar|standard|exir|all|BROKER_NAME> [test] [curl]",
                    args[0]
                ),
            );
            log_error(
                "CLI",
                "BROKER_NAME comes from config_standard.json or config_exir.json.",
            );
            log_error(
                "CLI",
                "The 'test' and 'curl' flags should come after the broker name.",
            );
            std::process::exit(1);
        }
        Some(other) => other,
        None => {
            log_error(
                "CLI",
                &format!(
                    "Usage: {} <mofid|danayan|bidar|standard|exir|all|BROKER_NAME> [test] [curl]",
                    args[0]
                ),
            );
            log_error(
                "CLI",
                "BROKER_NAME comes from config_standard.json or config_exir.json.",
            );
            std::process::exit(1);
        }
    };

    if test_mode {
        if curl_only {
            log_warn(
                "System",
                "*** TEST MODE + CURL ONLY: Will print curl commands without sending requests ***",
            );
        } else {
            log_warn(
                "System",
                "*** TEST MODE: Will send one order immediately without timers ***",
            );
        }
    }

    let global_config = global_config::load_config("config_global.json")?;
    if let Some(config) = global_config.as_ref() {
        time_reference::initialize(config.ntp.as_ref()).await?;
    } else {
        time_reference::initialize(None).await?;
    }

    match broker {
        "mofid" => run_mofid(test_mode, curl_only).await,
        "danayan" => run_danayan(test_mode, curl_only).await,
        "bidar" => run_bidar(test_mode, curl_only).await,
        "standard" => run_standard(test_mode, curl_only).await,
        "exir" => run_exir(test_mode, curl_only).await,
        "all" => run_all(test_mode, curl_only).await,
        other => match run_standard_broker_by_name(other, test_mode, curl_only).await {
            Ok(()) => Ok(()),
            Err(_) => run_exir_broker_by_name(other, test_mode, curl_only).await,
        },
    }
}

async fn run_all(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("System", "Starting Sarkhati - All Brokers in Parallel");

    let standard_config = standard_broker::load_config("config_standard.json")?;
    let exir_config = exir_broker::load_config("config_exir.json")?;

    let mofid_handle = tokio::spawn(async move {
        if let Err(e) = run_mofid(test_mode, curl_only).await {
            log_error("Mofid", &format!("Stopped with error: {}", e));
        }
    });

    let danayan_handle = tokio::spawn(async move {
        if let Err(e) = run_danayan(test_mode, curl_only).await {
            log_error("Danayan", &format!("Stopped with error: {}", e));
        }
    });

    let bidar_handle = tokio::spawn(async move {
        if let Err(e) = run_bidar(test_mode, curl_only).await {
            log_error("Bidar", &format!("Stopped with error: {}", e));
        }
    });

    let mut standard_handles = Vec::new();
    for broker in standard_config.accounts.clone() {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_standard_broker(broker, test_mode, curl_only).await {
                log_error("Standard", &format!("Stopped with error: {}", e));
            }
        });
        standard_handles.push(handle);
    }

    let mut exir_handles = Vec::new();
    for broker in exir_config.accounts.clone() {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_exir_broker(broker, test_mode, curl_only).await {
                log_error("Exir", &format!("Stopped with error: {}", e));
            }
        });
        exir_handles.push(handle);
    }

    let _ = tokio::join!(mofid_handle, danayan_handle, bidar_handle);
    for handle in standard_handles {
        let _ = handle.await;
    }
    for handle in exir_handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_standard(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("Standard", "Starting Sarkhati - Standard Brokers");

    let standard_config = standard_broker::load_config("config_standard.json")?;
    if standard_config.accounts.is_empty() {
        log_warn(
            "Standard",
            "No accounts found in config_standard.json; skipping.",
        );
        return Ok(());
    }

    let mut handles = Vec::new();
    for broker in standard_config.accounts {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_standard_broker(broker, test_mode, curl_only).await {
                log_error("Standard", &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_exir(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("Exir", "Starting Sarkhati - Exir Brokers");

    let exir_config = exir_broker::load_config("config_exir.json")?;
    if exir_config.accounts.is_empty() {
        log_warn("Exir", "No accounts found in config_exir.json; skipping.");
        return Ok(());
    }

    let mut handles = Vec::new();
    for broker in exir_config.accounts {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_exir_broker(broker, test_mode, curl_only).await {
                log_error("Exir", &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_standard_broker_by_name(name: &str, test_mode: bool, curl_only: bool) -> Result<()> {
    let config = standard_broker::load_config("config_standard.json")?;
    let broker = standard_broker::find_broker(&config, name)
        .cloned()
        .with_context(|| format!("Broker '{}' not found in config_standard.json", name))?;
    run_standard_broker(broker, test_mode, curl_only).await
}

async fn run_exir_broker_by_name(name: &str, test_mode: bool, curl_only: bool) -> Result<()> {
    let config = exir_broker::load_config("config_exir.json")?;
    let broker = exir_broker::find_broker(&config, name)
        .cloned()
        .with_context(|| format!("Broker '{}' not found in config_exir.json", name))?;
    run_exir_broker(broker, test_mode, curl_only).await
}

async fn run_standard_broker(
    broker: standard_broker::StandardBrokerConfig,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));

    log_info(
        &broker.name,
        &format!("Starting Sarkhati - {} Order Sender", broker.name),
    );

    if broker.cookie.is_empty() {
        anyhow::bail!(
            "Cookie is required for {}. Please set 'cookie' in config_standard.json",
            broker.name
        );
    }

    log_info(
        &broker.name,
        &format!(
            "Cookie auth enabled (preview: {}...)",
            &broker.cookie[..broker.cookie.len().min(50)]
        ),
    );

    if broker.orders.is_empty() {
        log_warn(&broker.name, "No orders configured; skipping.");
        return Ok(());
    }
    if broker.batch_repeat == 0 {
        log_warn(&broker.name, "batch_repeat is 0; skipping.");
        return Ok(());
    }

    if test_mode {
        log_info(
            &broker.name,
            "Test mode: sending one order immediately without scheduling.",
        );
        let order = broker
            .orders
            .first()
            .context("No orders available for test mode")?;
        let order_json = serde_json::to_string(order)?;
        standard_broker::send_order(
            &broker,
            &order_json,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        .with_context(|| format!("Failed to send test order for {}", broker.name))?;
        return Ok(());
    }

    if let Some(target_time_str) = &broker.target_time {
        log_info(
            &broker.name,
            &format!("Scheduled mode enabled for target time {}", target_time_str),
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let total_orders = broker.orders.len();
        let mut handles = Vec::new();
        for (order_index, order) in broker.orders.iter().enumerate() {
            let broker_clone = broker.clone();
            let order_json = serde_json::to_string(order)?;
            let broker_name = broker.name.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_standard_order_scheduled_task(
                    broker_clone,
                    order_json,
                    order_index,
                    total_orders,
                    target_time,
                    test_mode,
                    curl_only,
                )
                .await
                {
                    log_error(
                        &broker_name,
                        &format!("Order thread {} stopped: {}", order_index + 1, e),
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    log_info(
        &broker.name,
        &format!("Loaded {} order(s)", broker.orders.len()),
    );
    log_info(
        &broker.name,
        &format!(
            "Batch repeat: {} (delay {}ms between repeats)",
            broker.batch_repeat, broker.batch_delay_ms
        ),
    );
    log_info(&broker.name, "Starting continuous order sending...");

    let total_orders = broker.orders.len();
    let mut handles = Vec::new();
    for (order_index, order) in broker.orders.iter().enumerate() {
        let broker_clone = broker.clone();
        let order_json = serde_json::to_string(order)?;
        let broker_name = broker.name.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_standard_order_continuous(
                broker_clone,
                order_json,
                order_index,
                total_orders,
                test_mode,
                curl_only,
                limiter,
            )
            .await
            {
                log_error(
                    &broker_name,
                    &format!("Order thread {} stopped: {}", order_index + 1, e),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_exir_broker(
    broker: exir_broker::ExirBrokerConfig,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));

    log_info(
        &broker.name,
        &format!("Starting Sarkhati - {} Order Sender", broker.name),
    );

    if broker.cookie.is_empty() {
        anyhow::bail!(
            "Cookie is required for {}. Please set 'cookie' in config_exir.json",
            broker.name
        );
    }

    log_info(
        &broker.name,
        &format!(
            "Cookie auth enabled (preview: {}...)",
            &broker.cookie[..broker.cookie.len().min(50)]
        ),
    );

    if broker.orders.is_empty() {
        log_warn(&broker.name, "No orders configured; skipping.");
        return Ok(());
    }
    if broker.batch_repeat == 0 {
        log_warn(&broker.name, "batch_repeat is 0; skipping.");
        return Ok(());
    }

    if test_mode {
        log_info(
            &broker.name,
            "Test mode: sending one order immediately without scheduling.",
        );
        let order = broker
            .orders
            .first()
            .context("No orders available for test mode")?;
        let order_json = serde_json::to_string(order)?;
        exir_broker::send_order(
            &broker,
            &order_json,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        .with_context(|| format!("Failed to send test order for {}", broker.name))?;
        return Ok(());
    }

    if let Some(target_time_str) = &broker.target_time {
        log_info(
            &broker.name,
            &format!("Scheduled mode enabled for target time {}", target_time_str),
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let total_orders = broker.orders.len();
        let mut handles = Vec::new();
        for (order_index, order) in broker.orders.iter().enumerate() {
            let broker_clone = broker.clone();
            let order_json = serde_json::to_string(order)?;
            let broker_name = broker.name.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_exir_order_scheduled_task(
                    broker_clone,
                    order_json,
                    order_index,
                    total_orders,
                    target_time,
                    test_mode,
                    curl_only,
                )
                .await
                {
                    log_error(
                        &broker_name,
                        &format!("Order thread {} stopped: {}", order_index + 1, e),
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    log_info(
        &broker.name,
        &format!("Loaded {} order(s)", broker.orders.len()),
    );
    log_info(
        &broker.name,
        &format!(
            "Batch repeat: {} (delay {}ms between repeats)",
            broker.batch_repeat, broker.batch_delay_ms
        ),
    );
    log_info(&broker.name, "Starting continuous order sending...");

    let total_orders = broker.orders.len();
    let mut handles = Vec::new();
    for (order_index, order) in broker.orders.iter().enumerate() {
        let broker_clone = broker.clone();
        let order_json = serde_json::to_string(order)?;
        let broker_name = broker.name.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_exir_order_continuous(
                broker_clone,
                order_json,
                order_index,
                total_orders,
                test_mode,
                curl_only,
                limiter,
            )
            .await
            {
                log_error(
                    &broker_name,
                    &format!("Order thread {} stopped: {}", order_index + 1, e),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_mofid_account(
    config: mofid::MofidConfig,
    label: String,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    log_info(
        &label,
        &format!("Starting Sarkhati - {} Order Sender", label),
    );

    let use_cookie = !config.cookie.is_empty() && config.cookie != "PASTE_YOUR_COOKIE_HERE";
    let use_auth = !config.authorization.is_empty();

    if use_cookie {
        log_info(
            &label,
            &format!(
                "Cookie auth enabled (preview: {}...)",
                &config.cookie[..config.cookie.len().min(50)]
            ),
        );
    } else if use_auth {
        log_info(
            &label,
            &format!(
                "Authorization header enabled (preview: Bearer {}...)",
                &config.authorization[..config.authorization.len().min(30)]
            ),
        );
    } else {
        anyhow::bail!(
            "No authentication method configured. Please set either 'cookie' or 'authorization' in config.json"
        );
    }

    if config.orders.is_empty() {
        log_warn(&label, "No orders configured; skipping.");
        return Ok(());
    }
    if config.batch_repeat == 0 {
        log_warn(&label, "batch_repeat is 0; skipping.");
        return Ok(());
    }

    if test_mode {
        log_info(
            &label,
            "Test mode: sending one order immediately without scheduling.",
        );
        let order = config
            .orders
            .first()
            .context("No orders available for test mode")?;
        mofid::send_order(
            &config,
            order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        .with_context(|| format!("Failed to send test order for {}", label))?;
        return Ok(());
    }

    if let Some(target_time_str) = &config.target_time {
        log_info(
            &label,
            &format!("Scheduled mode enabled for target time {}", target_time_str),
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let total_orders = config.orders.len();
        let mut handles = Vec::new();
        for (order_index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let label_clone = label.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_mofid_order_scheduled_task(
                    config_clone,
                    order_clone,
                    label_clone.clone(),
                    order_index,
                    total_orders,
                    target_time,
                    test_mode,
                    curl_only,
                )
                .await
                {
                    log_error(
                        &label_clone,
                        &format!("Order thread {} stopped: {}", order_index + 1, e),
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    log_info(&label, &format!("Loaded {} order(s)", config.orders.len()));
    log_info(
        &label,
        &format!(
            "Batch repeat: {} (delay {}ms between repeats)",
            config.batch_repeat, config.batch_delay_ms
        ),
    );
    log_info(&label, "Starting continuous order sending...");

    let total_orders = config.orders.len();
    let mut handles = Vec::new();
    for (order_index, order) in config.orders.iter().enumerate() {
        let config_clone = config.clone();
        let order_clone = order.clone();
        let label_clone = label.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_mofid_order_continuous(
                config_clone,
                order_clone,
                label_clone.clone(),
                order_index,
                total_orders,
                test_mode,
                curl_only,
                limiter,
            )
            .await
            {
                log_error(
                    &label_clone,
                    &format!("Order thread {} stopped: {}", order_index + 1, e),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_mofid(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_mofid.json").context("Failed to read config_mofid.json")?;
    let config_file: mofid::MofidConfigFile =
        serde_json::from_str(&config_str).context("Failed to parse config_mofid.json")?;
    let accounts = match config_file {
        mofid::MofidConfigFile::Single(config) => vec![config],
        mofid::MofidConfigFile::Multiple { accounts } => accounts,
        mofid::MofidConfigFile::List(accounts) => accounts,
    };

    if accounts.is_empty() {
        log_warn("Mofid", "No accounts found in config_mofid.json; skipping.");
        return Ok(());
    }

    let mut handles = Vec::new();
    for (index, mut account) in accounts.into_iter().enumerate() {
        let fallback_label = format!("Mofid#{}", index + 1);
        let label = account.display_name(&fallback_label);
        if account.name.is_none() {
            account.name = Some(label.clone());
        }
        let handle = tokio::spawn(async move {
            if let Err(e) = run_mofid_account(account, label.clone(), test_mode, curl_only).await {
                log_error(&label, &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_danayan_account(
    config: danayan::DanayanConfig,
    label: String,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    log_info(
        &label,
        &format!("Starting Sarkhati - {} Order Sender", label),
    );

    if config.cookie.is_empty() {
        anyhow::bail!("Cookie is required for Danayan. Please set 'cookie' in config_danayan.json");
    }

    log_info(
        &label,
        &format!(
            "Cookie auth enabled (preview: {}...)",
            &config.cookie[..config.cookie.len().min(50)]
        ),
    );

    if config.orders.is_empty() {
        log_warn(&label, "No orders configured; skipping.");
        return Ok(());
    }
    if config.batch_repeat == 0 {
        log_warn(&label, "batch_repeat is 0; skipping.");
        return Ok(());
    }

    if test_mode {
        log_info(
            &label,
            "Test mode: sending one order immediately without scheduling.",
        );
        let order = config
            .orders
            .first()
            .context("No orders available for test mode")?;
        danayan::send_order(
            &config,
            order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        .with_context(|| format!("Failed to send test order for {}", label))?;
        return Ok(());
    }

    if let Some(target_time_str) = &config.target_time {
        log_info(
            &label,
            &format!("Scheduled mode enabled for target time {}", target_time_str),
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let total_orders = config.orders.len();
        let mut handles = Vec::new();
        for (order_index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let label_clone = label.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_danayan_order_scheduled_task(
                    config_clone,
                    order_clone,
                    label_clone.clone(),
                    order_index,
                    total_orders,
                    target_time,
                    test_mode,
                    curl_only,
                )
                .await
                {
                    log_error(
                        &label_clone,
                        &format!("Order thread {} stopped: {}", order_index + 1, e),
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    log_info(&label, &format!("Loaded {} order(s)", config.orders.len()));
    log_info(
        &label,
        &format!(
            "Batch repeat: {} (delay {}ms between repeats)",
            config.batch_repeat, config.batch_delay_ms
        ),
    );
    log_info(&label, "Starting continuous order sending...");

    let total_orders = config.orders.len();
    let mut handles = Vec::new();
    for (order_index, order) in config.orders.iter().enumerate() {
        let config_clone = config.clone();
        let order_clone = order.clone();
        let label_clone = label.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_danayan_order_continuous(
                config_clone,
                order_clone,
                label_clone.clone(),
                order_index,
                total_orders,
                test_mode,
                curl_only,
                limiter,
            )
            .await
            {
                log_error(
                    &label_clone,
                    &format!("Order thread {} stopped: {}", order_index + 1, e),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_danayan(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_danayan.json").context("Failed to read config_danayan.json")?;
    let config_file: danayan::DanayanConfigFile =
        serde_json::from_str(&config_str).context("Failed to parse config_danayan.json")?;
    let accounts = match config_file {
        danayan::DanayanConfigFile::Single(config) => vec![config],
        danayan::DanayanConfigFile::Multiple { accounts } => accounts,
        danayan::DanayanConfigFile::List(accounts) => accounts,
    };

    if accounts.is_empty() {
        log_warn(
            "Danayan",
            "No accounts found in config_danayan.json; skipping.",
        );
        return Ok(());
    }

    let mut handles = Vec::new();
    for (index, mut account) in accounts.into_iter().enumerate() {
        let fallback_label = format!("Danayan#{}", index + 1);
        let label = account.display_name(&fallback_label);
        if account.name.is_none() {
            account.name = Some(label.clone());
        }
        let handle = tokio::spawn(async move {
            if let Err(e) = run_danayan_account(account, label.clone(), test_mode, curl_only).await
            {
                log_error(&label, &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_bidar_account(
    config: bidar::BidarConfig,
    label: String,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    log_info(
        &label,
        &format!("Starting Sarkhati - {} Order Sender", label),
    );

    if config.authorization.is_empty() {
        anyhow::bail!(
            "Authorization token is required for Bidar. Please set 'authorization' in config_bidar.json"
        );
    }

    log_info(
        &label,
        &format!(
            "Bearer token auth enabled (preview: {}...)",
            &config.authorization[..config.authorization.len().min(50)]
        ),
    );

    if config.orders.is_empty() {
        log_warn(&label, "No orders configured; skipping.");
        return Ok(());
    }
    if config.batch_repeat == 0 {
        log_warn(&label, "batch_repeat is 0; skipping.");
        return Ok(());
    }

    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    if test_mode {
        log_info(
            &label,
            "Test mode: sending one order immediately without scheduling.",
        );
        let order = config
            .orders
            .first()
            .context("No orders available for test mode")?;
        bidar::send_order(
            &config,
            order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        .with_context(|| format!("Failed to send test order for {}", label))?;
        return Ok(());
    }

    if let Some(target_time_str) = &config.target_time {
        log_info(
            &label,
            &format!("Scheduled mode enabled for target time {}", target_time_str),
        );
        let target_time = chrono::NaiveTime::parse_from_str(target_time_str, "%H:%M:%S%.3f")
            .context("target_time must be in HH:MM:SS.mmm format")?;
        let total_orders = config.orders.len();
        let mut handles = Vec::new();
        for (order_index, order) in config.orders.iter().enumerate() {
            let config_clone = config.clone();
            let order_clone = order.clone();
            let label_clone = label.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_bidar_order_scheduled_task(
                    config_clone,
                    order_clone,
                    label_clone.clone(),
                    order_index,
                    total_orders,
                    target_time,
                    test_mode,
                    curl_only,
                )
                .await
                {
                    log_error(
                        &label_clone,
                        &format!("Order thread {} stopped: {}", order_index + 1, e),
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }

    log_info(&label, &format!("Loaded {} order(s)", config.orders.len()));
    log_info(
        &label,
        &format!(
            "Batch repeat: {} (delay {}ms between repeats)",
            config.batch_repeat, config.batch_delay_ms
        ),
    );
    log_info(&label, "Starting continuous order sending...");

    let total_orders = config.orders.len();
    let mut handles = Vec::new();
    for (order_index, order) in config.orders.iter().enumerate() {
        let config_clone = config.clone();
        let order_clone = order.clone();
        let label_clone = label.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_bidar_order_continuous(
                config_clone,
                order_clone,
                label_clone.clone(),
                order_index,
                total_orders,
                test_mode,
                curl_only,
                limiter,
            )
            .await
            {
                log_error(
                    &label_clone,
                    &format!("Order thread {} stopped: {}", order_index + 1, e),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_bidar(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str =
        fs::read_to_string("config_bidar.json").context("Failed to read config_bidar.json")?;
    let config_file: bidar::BidarConfigFile =
        serde_json::from_str(&config_str).context("Failed to parse config_bidar.json")?;
    let accounts = match config_file {
        bidar::BidarConfigFile::Single(config) => vec![config],
        bidar::BidarConfigFile::Multiple { accounts } => accounts,
        bidar::BidarConfigFile::List(accounts) => accounts,
    };

    if accounts.is_empty() {
        log_warn("Bidar", "No accounts found in config_bidar.json; skipping.");
        return Ok(());
    }

    let mut handles = Vec::new();
    for (index, mut account) in accounts.into_iter().enumerate() {
        let fallback_label = format!("Bidar#{}", index + 1);
        let label = account.display_name(&fallback_label);
        if account.name.is_none() {
            account.name = Some(label.clone());
        }
        let handle = tokio::spawn(async move {
            if let Err(e) = run_bidar_account(account, label.clone(), test_mode, curl_only).await {
                log_error(&label, &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_standard_order_schedule(
    broker: standard_broker::StandardBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    let mut last_wall_epoch_ms = current_epoch_millis()?;
    for repeat_index in 0..broker.batch_repeat {
        let scheduled_epoch_ms =
            final_send_epoch_ms + repeat_index as i64 * broker.batch_delay_ms as i64;
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms > scheduled_epoch_ms {
            log_warn(
                &broker.name,
                &format!(
                    "Order {}/{} scheduled send time passed by {}ms for batch #{}",
                    order_index + 1,
                    total_orders,
                    now_epoch_ms - scheduled_epoch_ms,
                    repeat_index + 1
                ),
            );
        }
        wait_until_epoch_ms(scheduled_epoch_ms, &mut last_wall_epoch_ms).await?;

        let actual_send_time = time_reference::now_utc().with_timezone(&Tehran);
        let actual_epoch_us = current_epoch_micros()?;
        let drift_micros = actual_epoch_us - scheduled_epoch_ms as i128 * 1_000;
        log_info(
            &broker.name,
            &format!(
                "Sending scheduled batch #{} order {}/{} at {} (drift {}µs, epoch_us={})",
                repeat_index + 1,
                order_index + 1,
                total_orders,
                actual_send_time.format("%H:%M:%S%.3f"),
                drift_micros,
                actual_epoch_us
            ),
        );

        match standard_broker::send_order(
            &broker,
            &order_json,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        {
            Ok(_) => log_success(
                &broker.name,
                &format!(
                    "Batch {}/{} Order {}/{} sent",
                    repeat_index + 1,
                    broker.batch_repeat,
                    order_index + 1,
                    total_orders
                ),
            ),
            Err(e) => log_error(
                &broker.name,
                &format!(
                    "Batch {}/{} Order {}/{} failed: {}",
                    repeat_index + 1,
                    broker.batch_repeat,
                    order_index + 1,
                    total_orders,
                    e
                ),
            ),
        }
    }
    Ok(())
}

async fn run_standard_order_scheduled_task(
    broker: standard_broker::StandardBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    target_time: chrono::NaiveTime,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let calibration_enabled = broker
        .calibration
        .as_ref()
        .map_or(false, |calibration| calibration.enabled);
    let client = reqwest::Client::new();
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));

    loop {
        let target_datetime = next_target_datetime(target_time)?;
        let target_epoch_ms = target_datetime.timestamp_millis();
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms < target_epoch_ms {
            log_info(
                &broker.name,
                &format!(
                    "Next target_time={} (epoch_ms={})",
                    target_datetime.format("%Y-%m-%d %H:%M:%S%.3f"),
                    target_epoch_ms
                ),
            );
        }

        let mut calibration_deadline_epoch_ms = None;
        if calibration_enabled {
            let calibration = broker
                .calibration
                .as_ref()
                .context("Calibration config missing")?;
            let calibration_deadline =
                target_epoch_ms - calibration.calibration_start_margin_ms as i64;
            calibration_deadline_epoch_ms = Some(calibration_deadline);
            let calibration_start_epoch_ms =
                calibration_deadline - calibration.calibration_window_ms as i64;
            if now_epoch_ms < calibration_start_epoch_ms {
                let sleep_ms = calibration_start_epoch_ms - now_epoch_ms;
                log_info(
                    &broker.name,
                    &format!(
                        "Waiting {}ms before calibration window (epoch_ms={})",
                        sleep_ms, calibration_start_epoch_ms
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;
            }
            let now_epoch_ms = current_epoch_millis()?;
            if now_epoch_ms > calibration_deadline {
                log_warn(
                    &broker.name,
                    "Too late to calibrate before target_time; proceeding with available data",
                );
            }
        }

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = standard_broker::run_calibration(
                &broker,
                &client,
                rate_limiter.as_ref(),
                calibration_deadline_epoch_ms,
            )
            .await?;
            (
                summary.estimated_delay_ms,
                broker
                    .calibration
                    .as_ref()
                    .map(|calibration| calibration.safety_margin_ms)
                    .unwrap_or_default(),
                summary.last_probe_wall_time,
            )
        } else {
            log_info(
                &broker.name,
                "Calibration disabled; using zero delay estimate.",
            );
            (0, 0, time_reference::now_system_time())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_epoch_ms = target_epoch_ms - effective_delay_ms as i64;
        let final_send_time = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(final_send_epoch_ms as u64),
        )
        .with_timezone(&Tehran);

        let now_epoch_ms = current_epoch_millis()?;
        if final_send_epoch_ms <= now_epoch_ms {
            log_warn(
                &broker.name,
                "final_send_time has already passed; sending as soon as possible",
            );
        }

        if calibration_enabled {
            let last_probe_epoch_ms = last_probe_wall_time
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as i64;
            let gap_ms = final_send_epoch_ms - last_probe_epoch_ms;
            if gap_ms < broker.batch_delay_ms as i64 {
                log_warn(
                    &broker.name,
                    &format!(
                        "Last probe is too close to final_send_time; gap {}ms < {}ms",
                        gap_ms, broker.batch_delay_ms
                    ),
                );
            }
        }

        log_info(
            &broker.name,
            &format!(
                "target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
                target_datetime.format("%H:%M:%S%.3f"),
                final_send_time.format("%H:%M:%S%.3f"),
                estimated_delay_ms,
                safety_margin_ms,
                effective_delay_ms
            ),
        );
        log_info(
            &broker.name,
            &format!(
                "target_epoch_ms={} final_send_epoch_ms={}",
                target_epoch_ms, final_send_epoch_ms
            ),
        );

        run_standard_order_schedule(
            broker.clone(),
            order_json.clone(),
            order_index,
            total_orders,
            final_send_epoch_ms,
            test_mode,
            curl_only,
            rate_limiter.clone(),
        )
        .await?;
    }
}

async fn run_standard_order_continuous(
    broker: standard_broker::StandardBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        for repeat_index in 0..broker.batch_repeat {
            if repeat_index > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(broker.batch_delay_ms)).await;
            }

            match standard_broker::send_order(
                &broker,
                &order_json,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            {
                Ok(_) => log_success(
                    &broker.name,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        broker.batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &broker.name,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        broker.batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }

            if test_mode {
                log_info(&broker.name, "Test mode: exiting after one batch cycle");
                return Ok(());
            }
        }
    }
}

async fn run_exir_order_schedule(
    broker: exir_broker::ExirBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    let mut last_wall_epoch_ms = current_epoch_millis()?;
    for repeat_index in 0..broker.batch_repeat {
        let scheduled_epoch_ms =
            final_send_epoch_ms + repeat_index as i64 * broker.batch_delay_ms as i64;
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms > scheduled_epoch_ms {
            log_warn(
                &broker.name,
                &format!(
                    "Order {}/{} scheduled send time passed by {}ms for batch #{}",
                    order_index + 1,
                    total_orders,
                    now_epoch_ms - scheduled_epoch_ms,
                    repeat_index + 1
                ),
            );
        }
        wait_until_epoch_ms(scheduled_epoch_ms, &mut last_wall_epoch_ms).await?;

        let actual_send_time = time_reference::now_utc().with_timezone(&Tehran);
        let actual_epoch_us = current_epoch_micros()?;
        let drift_micros = actual_epoch_us - scheduled_epoch_ms as i128 * 1_000;
        log_info(
            &broker.name,
            &format!(
                "Sending scheduled batch #{} order {}/{} at {} (drift {}µs, epoch_us={})",
                repeat_index + 1,
                order_index + 1,
                total_orders,
                actual_send_time.format("%H:%M:%S%.3f"),
                drift_micros,
                actual_epoch_us
            ),
        );

        match exir_broker::send_order(
            &broker,
            &order_json,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        {
            Ok(_) => log_success(
                &broker.name,
                &format!(
                    "Batch {}/{} Order {}/{} sent",
                    repeat_index + 1,
                    broker.batch_repeat,
                    order_index + 1,
                    total_orders
                ),
            ),
            Err(e) => log_error(
                &broker.name,
                &format!(
                    "Batch {}/{} Order {}/{} failed: {}",
                    repeat_index + 1,
                    broker.batch_repeat,
                    order_index + 1,
                    total_orders,
                    e
                ),
            ),
        }
    }
    Ok(())
}

async fn run_exir_order_scheduled_task(
    broker: exir_broker::ExirBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    target_time: chrono::NaiveTime,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let calibration_enabled = broker
        .calibration
        .as_ref()
        .map_or(false, |calibration| calibration.enabled);
    let client = reqwest::Client::new();
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));

    loop {
        let target_datetime = next_target_datetime(target_time)?;
        let target_epoch_ms = target_datetime.timestamp_millis();
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms < target_epoch_ms {
            log_info(
                &broker.name,
                &format!(
                    "Next target_time={} (epoch_ms={})",
                    target_datetime.format("%Y-%m-%d %H:%M:%S%.3f"),
                    target_epoch_ms
                ),
            );
        }

        let mut calibration_deadline_epoch_ms = None;
        if calibration_enabled {
            let calibration = broker
                .calibration
                .as_ref()
                .context("Calibration config missing")?;
            let calibration_deadline =
                target_epoch_ms - calibration.calibration_start_margin_ms as i64;
            calibration_deadline_epoch_ms = Some(calibration_deadline);
            let calibration_start_epoch_ms =
                calibration_deadline - calibration.calibration_window_ms as i64;
            if now_epoch_ms < calibration_start_epoch_ms {
                let sleep_ms = calibration_start_epoch_ms - now_epoch_ms;
                log_info(
                    &broker.name,
                    &format!(
                        "Waiting {}ms before calibration window (epoch_ms={})",
                        sleep_ms, calibration_start_epoch_ms
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;
            }
            let now_epoch_ms = current_epoch_millis()?;
            if now_epoch_ms > calibration_deadline {
                log_warn(
                    &broker.name,
                    "Too late to calibrate before target_time; proceeding with available data",
                );
            }
        }

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = exir_broker::run_calibration(
                &broker,
                &client,
                rate_limiter.as_ref(),
                calibration_deadline_epoch_ms,
            )
            .await?;
            (
                summary.estimated_delay_ms,
                broker
                    .calibration
                    .as_ref()
                    .map(|calibration| calibration.safety_margin_ms)
                    .unwrap_or_default(),
                summary.last_probe_wall_time,
            )
        } else {
            log_info(
                &broker.name,
                "Calibration disabled; using zero delay estimate.",
            );
            (0, 0, time_reference::now_system_time())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_epoch_ms = target_epoch_ms - effective_delay_ms as i64;
        let final_send_time = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(final_send_epoch_ms as u64),
        )
        .with_timezone(&Tehran);

        let now_epoch_ms = current_epoch_millis()?;
        if final_send_epoch_ms <= now_epoch_ms {
            log_warn(
                &broker.name,
                "final_send_time has already passed; sending as soon as possible",
            );
        }

        if calibration_enabled {
            let last_probe_epoch_ms = last_probe_wall_time
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as i64;
            let gap_ms = final_send_epoch_ms - last_probe_epoch_ms;
            if gap_ms < broker.batch_delay_ms as i64 {
                log_warn(
                    &broker.name,
                    &format!(
                        "Last probe is too close to final_send_time; gap {}ms < {}ms",
                        gap_ms, broker.batch_delay_ms
                    ),
                );
            }
        }

        log_info(
            &broker.name,
            &format!(
                "target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
                target_datetime.format("%H:%M:%S%.3f"),
                final_send_time.format("%H:%M:%S%.3f"),
                estimated_delay_ms,
                safety_margin_ms,
                effective_delay_ms
            ),
        );
        log_info(
            &broker.name,
            &format!(
                "target_epoch_ms={} final_send_epoch_ms={}",
                target_epoch_ms, final_send_epoch_ms
            ),
        );

        run_exir_order_schedule(
            broker.clone(),
            order_json.clone(),
            order_index,
            total_orders,
            final_send_epoch_ms,
            test_mode,
            curl_only,
            rate_limiter.clone(),
        )
        .await?;
    }
}

async fn run_exir_order_continuous(
    broker: exir_broker::ExirBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        for repeat_index in 0..broker.batch_repeat {
            if repeat_index > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(broker.batch_delay_ms)).await;
            }

            match exir_broker::send_order(
                &broker,
                &order_json,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            {
                Ok(_) => log_success(
                    &broker.name,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        broker.batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &broker.name,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        broker.batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }

            if test_mode {
                log_info(&broker.name, "Test mode: exiting after one batch cycle");
                return Ok(());
            }
        }
    }
}

async fn run_mofid_order_schedule(
    config: mofid::MofidConfig,
    order: mofid::MofidOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    let mut last_wall_epoch_ms = current_epoch_millis()?;
    for repeat_index in 0..config.batch_repeat {
        let scheduled_epoch_ms =
            final_send_epoch_ms + repeat_index as i64 * config.batch_delay_ms as i64;
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms > scheduled_epoch_ms {
            log_warn(
                &label,
                &format!(
                    "Order {}/{} scheduled send time passed by {}ms for batch #{}",
                    order_index + 1,
                    total_orders,
                    now_epoch_ms - scheduled_epoch_ms,
                    repeat_index + 1
                ),
            );
        }
        wait_until_epoch_ms(scheduled_epoch_ms, &mut last_wall_epoch_ms).await?;

        let actual_send_time = time_reference::now_utc().with_timezone(&Tehran);
        let actual_epoch_us = current_epoch_micros()?;
        let drift_micros = actual_epoch_us - scheduled_epoch_ms as i128 * 1_000;
        log_info(
            &label,
            &format!(
                "Sending scheduled batch #{} order {}/{} at {} (drift {}µs, epoch_us={})",
                repeat_index + 1,
                order_index + 1,
                total_orders,
                actual_send_time.format("%H:%M:%S%.3f"),
                drift_micros,
                actual_epoch_us
            ),
        );

        match mofid::send_order(
            &config,
            &order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        {
            Ok(_) => log_success(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} sent",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders
                ),
            ),
            Err(e) => log_error(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} failed: {}",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders,
                    e
                ),
            ),
        }
    }
    Ok(())
}

async fn run_mofid_order_scheduled_task(
    config: mofid::MofidConfig,
    order: mofid::MofidOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    target_time: chrono::NaiveTime,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let calibration_enabled = config
        .calibration
        .as_ref()
        .map_or(false, |calibration| calibration.enabled);
    let client = reqwest::Client::new();
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    loop {
        let target_datetime = next_target_datetime(target_time)?;
        let target_epoch_ms = target_datetime.timestamp_millis();
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms < target_epoch_ms {
            log_info(
                &label,
                &format!(
                    "Next target_time={} (epoch_ms={})",
                    target_datetime.format("%Y-%m-%d %H:%M:%S%.3f"),
                    target_epoch_ms
                ),
            );
        }

        let mut calibration_deadline_epoch_ms = None;
        if calibration_enabled {
            let calibration = config
                .calibration
                .as_ref()
                .context("Calibration config missing")?;
            let calibration_deadline =
                target_epoch_ms - calibration.calibration_start_margin_ms as i64;
            calibration_deadline_epoch_ms = Some(calibration_deadline);
            let calibration_start_epoch_ms =
                calibration_deadline - calibration.calibration_window_ms as i64;
            if now_epoch_ms < calibration_start_epoch_ms {
                let sleep_ms = calibration_start_epoch_ms - now_epoch_ms;
                log_info(
                    &label,
                    &format!(
                        "Waiting {}ms before calibration window (epoch_ms={})",
                        sleep_ms, calibration_start_epoch_ms
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;
            }
            let now_epoch_ms = current_epoch_millis()?;
            if now_epoch_ms > calibration_deadline {
                log_warn(
                    &label,
                    "Too late to calibrate before target_time; proceeding with available data",
                );
            }
        }

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = mofid::run_calibration(
                &config,
                &client,
                rate_limiter.as_ref(),
                calibration_deadline_epoch_ms,
            )
            .await?;
            (
                summary.estimated_delay_ms,
                config
                    .calibration
                    .as_ref()
                    .map(|calibration| calibration.safety_margin_ms)
                    .unwrap_or_default(),
                summary.last_probe_wall_time,
            )
        } else {
            log_info(&label, "Calibration disabled; using zero delay estimate.");
            (0, 0, time_reference::now_system_time())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_epoch_ms = target_epoch_ms - effective_delay_ms as i64;
        let final_send_time = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(final_send_epoch_ms as u64),
        )
        .with_timezone(&Tehran);

        let now_epoch_ms = current_epoch_millis()?;
        if final_send_epoch_ms <= now_epoch_ms {
            log_warn(
                &label,
                "final_send_time has already passed; sending as soon as possible",
            );
        }

        if calibration_enabled {
            let last_probe_epoch_ms = last_probe_wall_time
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as i64;
            let gap_ms = final_send_epoch_ms - last_probe_epoch_ms;
            if gap_ms < config.batch_delay_ms as i64 {
                log_warn(
                    &label,
                    &format!(
                        "Last probe is too close to final_send_time; gap {}ms < {}ms",
                        gap_ms, config.batch_delay_ms
                    ),
                );
            }
        }

        log_info(
            &label,
            &format!(
                "target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
                target_datetime.format("%H:%M:%S%.3f"),
                final_send_time.format("%H:%M:%S%.3f"),
                estimated_delay_ms,
                safety_margin_ms,
                effective_delay_ms
            ),
        );
        log_info(
            &label,
            &format!(
                "target_epoch_ms={} final_send_epoch_ms={}",
                target_epoch_ms, final_send_epoch_ms
            ),
        );

        run_mofid_order_schedule(
            config.clone(),
            order.clone(),
            label.clone(),
            order_index,
            total_orders,
            final_send_epoch_ms,
            test_mode,
            curl_only,
            rate_limiter.clone(),
        )
        .await?;
    }
}

async fn run_mofid_order_continuous(
    config: mofid::MofidConfig,
    order: mofid::MofidOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        for repeat_index in 0..config.batch_repeat {
            if repeat_index > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(config.batch_delay_ms)).await;
            }

            match mofid::send_order(
                &config,
                &order,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            {
                Ok(_) => log_success(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }

            if test_mode {
                log_info(&label, "Test mode: exiting after one batch cycle");
                return Ok(());
            }
        }
    }
}

async fn run_danayan_order_schedule(
    config: danayan::DanayanConfig,
    order: danayan::DanayanOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    let mut last_wall_epoch_ms = current_epoch_millis()?;
    for repeat_index in 0..config.batch_repeat {
        let scheduled_epoch_ms =
            final_send_epoch_ms + repeat_index as i64 * config.batch_delay_ms as i64;
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms > scheduled_epoch_ms {
            log_warn(
                &label,
                &format!(
                    "Order {}/{} scheduled send time passed by {}ms for batch #{}",
                    order_index + 1,
                    total_orders,
                    now_epoch_ms - scheduled_epoch_ms,
                    repeat_index + 1
                ),
            );
        }
        wait_until_epoch_ms(scheduled_epoch_ms, &mut last_wall_epoch_ms).await?;

        let actual_send_time = time_reference::now_utc().with_timezone(&Tehran);
        let actual_epoch_us = current_epoch_micros()?;
        let drift_micros = actual_epoch_us - scheduled_epoch_ms as i128 * 1_000;
        log_info(
            &label,
            &format!(
                "Sending scheduled batch #{} order {}/{} at {} (drift {}µs, epoch_us={})",
                repeat_index + 1,
                order_index + 1,
                total_orders,
                actual_send_time.format("%H:%M:%S%.3f"),
                drift_micros,
                actual_epoch_us
            ),
        );

        match danayan::send_order(
            &config,
            &order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        {
            Ok(_) => log_success(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} sent",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders
                ),
            ),
            Err(e) => log_error(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} failed: {}",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders,
                    e
                ),
            ),
        }
    }
    Ok(())
}

async fn run_danayan_order_scheduled_task(
    config: danayan::DanayanConfig,
    order: danayan::DanayanOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    target_time: chrono::NaiveTime,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let calibration_enabled = config
        .calibration
        .as_ref()
        .map_or(false, |calibration| calibration.enabled);
    let client = reqwest::Client::new();
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    loop {
        let target_datetime = next_target_datetime(target_time)?;
        let target_epoch_ms = target_datetime.timestamp_millis();
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms < target_epoch_ms {
            log_info(
                &label,
                &format!(
                    "Next target_time={} (epoch_ms={})",
                    target_datetime.format("%Y-%m-%d %H:%M:%S%.3f"),
                    target_epoch_ms
                ),
            );
        }

        let mut calibration_deadline_epoch_ms = None;
        if calibration_enabled {
            let calibration = config
                .calibration
                .as_ref()
                .context("Calibration config missing")?;
            let calibration_deadline =
                target_epoch_ms - calibration.calibration_start_margin_ms as i64;
            calibration_deadline_epoch_ms = Some(calibration_deadline);
            let calibration_start_epoch_ms =
                calibration_deadline - calibration.calibration_window_ms as i64;
            if now_epoch_ms < calibration_start_epoch_ms {
                let sleep_ms = calibration_start_epoch_ms - now_epoch_ms;
                log_info(
                    &label,
                    &format!(
                        "Waiting {}ms before calibration window (epoch_ms={})",
                        sleep_ms, calibration_start_epoch_ms
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;
            }
            let now_epoch_ms = current_epoch_millis()?;
            if now_epoch_ms > calibration_deadline {
                log_warn(
                    &label,
                    "Too late to calibrate before target_time; proceeding with available data",
                );
            }
        }

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = danayan::run_calibration(
                &config,
                &client,
                rate_limiter.as_ref(),
                calibration_deadline_epoch_ms,
            )
            .await?;
            (
                summary.estimated_delay_ms,
                config
                    .calibration
                    .as_ref()
                    .map(|calibration| calibration.safety_margin_ms)
                    .unwrap_or_default(),
                summary.last_probe_wall_time,
            )
        } else {
            log_info(&label, "Calibration disabled; using zero delay estimate.");
            (0, 0, time_reference::now_system_time())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_epoch_ms = target_epoch_ms - effective_delay_ms as i64;
        let final_send_time = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(final_send_epoch_ms as u64),
        )
        .with_timezone(&Tehran);

        let now_epoch_ms = current_epoch_millis()?;
        if final_send_epoch_ms <= now_epoch_ms {
            log_warn(
                &label,
                "final_send_time has already passed; sending as soon as possible",
            );
        }

        if calibration_enabled {
            let last_probe_epoch_ms = last_probe_wall_time
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as i64;
            let gap_ms = final_send_epoch_ms - last_probe_epoch_ms;
            if gap_ms < config.batch_delay_ms as i64 {
                log_warn(
                    &label,
                    &format!(
                        "Last probe is too close to final_send_time; gap {}ms < {}ms",
                        gap_ms, config.batch_delay_ms
                    ),
                );
            }
        }

        log_info(
            &label,
            &format!(
                "target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
                target_datetime.format("%H:%M:%S%.3f"),
                final_send_time.format("%H:%M:%S%.3f"),
                estimated_delay_ms,
                safety_margin_ms,
                effective_delay_ms
            ),
        );
        log_info(
            &label,
            &format!(
                "target_epoch_ms={} final_send_epoch_ms={}",
                target_epoch_ms, final_send_epoch_ms
            ),
        );

        run_danayan_order_schedule(
            config.clone(),
            order.clone(),
            label.clone(),
            order_index,
            total_orders,
            final_send_epoch_ms,
            test_mode,
            curl_only,
            rate_limiter.clone(),
        )
        .await?;
    }
}

async fn run_danayan_order_continuous(
    config: danayan::DanayanConfig,
    order: danayan::DanayanOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        for repeat_index in 0..config.batch_repeat {
            if repeat_index > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(config.batch_delay_ms)).await;
            }

            match danayan::send_order(
                &config,
                &order,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            {
                Ok(_) => log_success(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }

            if test_mode {
                log_info(&label, "Test mode: exiting after one batch cycle");
                return Ok(());
            }
        }
    }
}

async fn run_bidar_order_schedule(
    config: bidar::BidarConfig,
    order: bidar::BidarOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    let mut last_wall_epoch_ms = current_epoch_millis()?;
    for repeat_index in 0..config.batch_repeat {
        let scheduled_epoch_ms =
            final_send_epoch_ms + repeat_index as i64 * config.batch_delay_ms as i64;
        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms > scheduled_epoch_ms {
            log_warn(
                &label,
                &format!(
                    "Order {}/{} scheduled send time passed by {}ms for batch #{}",
                    order_index + 1,
                    total_orders,
                    now_epoch_ms - scheduled_epoch_ms,
                    repeat_index + 1
                ),
            );
        }
        wait_until_epoch_ms(scheduled_epoch_ms, &mut last_wall_epoch_ms).await?;

        let actual_send_time = time_reference::now_utc().with_timezone(&Tehran);
        let actual_epoch_us = current_epoch_micros()?;
        let drift_micros = actual_epoch_us - scheduled_epoch_ms as i128 * 1_000;
        log_info(
            &label,
            &format!(
                "Sending scheduled batch #{} order {}/{} at {} (drift {}µs, epoch_us={})",
                repeat_index + 1,
                order_index + 1,
                total_orders,
                actual_send_time.format("%H:%M:%S%.3f"),
                drift_micros,
                actual_epoch_us
            ),
        );

        match bidar::send_order(
            &config,
            &order,
            test_mode,
            curl_only,
            Some(rate_limiter.as_ref()),
        )
        .await
        {
            Ok(_) => log_success(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} sent",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders
                ),
            ),
            Err(e) => log_error(
                &label,
                &format!(
                    "Batch {}/{} Order {}/{} failed: {}",
                    repeat_index + 1,
                    config.batch_repeat,
                    order_index + 1,
                    total_orders,
                    e
                ),
            ),
        }
    }
    Ok(())
}

async fn run_bidar_order_scheduled_task(
    config: bidar::BidarConfig,
    order: bidar::BidarOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    target_time: chrono::NaiveTime,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let calibration_enabled = config
        .calibration
        .as_ref()
        .map_or(false, |calibration| calibration.enabled);
    let client = reqwest::Client::new();
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(config.batch_delay_ms));

    loop {
        let target_datetime = next_target_datetime(target_time)?;
        let target_epoch_ms = target_datetime.timestamp_millis();

        let now_epoch_ms = current_epoch_millis()?;
        if now_epoch_ms < target_epoch_ms {
            log_info(
                &label,
                &format!(
                    "Next target_time={} (epoch_ms={})",
                    target_datetime.format("%Y-%m-%d %H:%M:%S%.3f"),
                    target_epoch_ms
                ),
            );
        }

        let mut calibration_deadline_epoch_ms = None;
        if calibration_enabled {
            let calibration = config
                .calibration
                .as_ref()
                .context("Calibration config missing")?;
            let calibration_deadline =
                target_epoch_ms - calibration.calibration_start_margin_ms as i64;
            calibration_deadline_epoch_ms = Some(calibration_deadline);
            let calibration_start_epoch_ms =
                calibration_deadline - calibration.calibration_window_ms as i64;
            if now_epoch_ms < calibration_start_epoch_ms {
                let sleep_ms = calibration_start_epoch_ms - now_epoch_ms;
                log_info(
                    &label,
                    &format!(
                        "Waiting {}ms before calibration window (epoch_ms={})",
                        sleep_ms, calibration_start_epoch_ms
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms as u64)).await;
            }
            let now_epoch_ms = current_epoch_millis()?;
            if now_epoch_ms > calibration_deadline {
                log_warn(
                    &label,
                    "Too late to calibrate before target_time; proceeding with available data",
                );
            }
        }

        let (estimated_delay_ms, safety_margin_ms, last_probe_wall_time) = if calibration_enabled {
            let summary = bidar::run_calibration(
                &config,
                &client,
                rate_limiter.as_ref(),
                calibration_deadline_epoch_ms,
            )
            .await?;
            let mut estimated_delay_ms = summary.estimated_delay_ms;
            match config.delay_model {
                bidar::BidarDelayModel::Rtt => {}
                bidar::BidarDelayModel::HalfRtt => {
                    estimated_delay_ms = (estimated_delay_ms + 1) / 2;
                    log_info(
                        &label,
                        &format!(
                            "Delay model half_rtt applied, estimate now {}ms",
                            estimated_delay_ms
                        ),
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
            log_info(&label, "Calibration disabled; using zero delay estimate.");
            (0, 0, time_reference::now_system_time())
        };

        let effective_delay_ms = estimated_delay_ms + safety_margin_ms;
        let final_send_epoch_ms = target_epoch_ms - effective_delay_ms as i64;
        let final_send_time = chrono::DateTime::<chrono::Utc>::from(
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(final_send_epoch_ms as u64),
        )
        .with_timezone(&Tehran);

        let now_epoch_ms = current_epoch_millis()?;
        if final_send_epoch_ms <= now_epoch_ms {
            log_warn(
                &label,
                "final_send_time has already passed; sending as soon as possible",
            );
        }

        if calibration_enabled {
            let last_probe_epoch_ms = last_probe_wall_time
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as i64;
            let gap_ms = final_send_epoch_ms - last_probe_epoch_ms;
            if gap_ms < config.batch_delay_ms as i64 {
                log_warn(
                    &label,
                    &format!(
                        "Last probe is too close to final_send_time; gap {}ms < {}ms",
                        gap_ms, config.batch_delay_ms
                    ),
                );
            }
        }

        log_info(
            &label,
            &format!(
                "target_time={} final_send_time={} estimator_delay={}ms safety_margin={}ms effective_delay={}ms",
                target_datetime.format("%H:%M:%S%.3f"),
                final_send_time.format("%H:%M:%S%.3f"),
                estimated_delay_ms,
                safety_margin_ms,
                effective_delay_ms
            ),
        );
        log_info(
            &label,
            &format!(
                "target_epoch_ms={} final_send_epoch_ms={}",
                target_epoch_ms, final_send_epoch_ms
            ),
        );

        run_bidar_order_schedule(
            config.clone(),
            order.clone(),
            label.clone(),
            order_index,
            total_orders,
            final_send_epoch_ms,
            test_mode,
            curl_only,
            rate_limiter.clone(),
        )
        .await?;
    }
}

async fn run_bidar_order_continuous(
    config: bidar::BidarConfig,
    order: bidar::BidarOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        for repeat_index in 0..config.batch_repeat {
            if repeat_index > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(config.batch_delay_ms)).await;
            }

            match bidar::send_order(
                &config,
                &order,
                test_mode,
                curl_only,
                Some(rate_limiter.as_ref()),
            )
            .await
            {
                Ok(_) => log_success(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        config.batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }

            if test_mode {
                log_info(&label, "Test mode: exiting after one batch cycle");
                return Ok(());
            }
        }
    }
}

fn next_target_datetime(target_time: chrono::NaiveTime) -> Result<chrono::DateTime<chrono_tz::Tz>> {
    let now = time_reference::now_utc().with_timezone(&Tehran);
    let today = now.date_naive();
    let candidate = Tehran
        .from_local_datetime(&today.and_time(target_time))
        .single()
        .context("Failed to resolve target_time in Asia/Tehran timezone")?;
    if candidate > now {
        Ok(candidate)
    } else {
        Ok(candidate + chrono::Duration::days(1))
    }
}

fn current_epoch_millis() -> Result<i64> {
    let now = time_reference::now_system_time()
        .duration_since(std::time::UNIX_EPOCH)
        .context("System time is before UNIX_EPOCH")?;
    Ok(now.as_millis() as i64)
}

fn current_epoch_micros() -> Result<i128> {
    let now = time_reference::now_system_time()
        .duration_since(std::time::UNIX_EPOCH)
        .context("System time is before UNIX_EPOCH")?;
    Ok(now.as_micros() as i128)
}

async fn wait_until_epoch_ms(target_epoch_ms: i64, last_wall_epoch_ms: &mut i64) -> Result<()> {
    let now_epoch_ms = current_epoch_millis()?;
    if now_epoch_ms < *last_wall_epoch_ms {
        anyhow::bail!("System clock moved backwards; aborting");
    }
    *last_wall_epoch_ms = now_epoch_ms;

    let until_target_ms = target_epoch_ms - now_epoch_ms;
    let spin_threshold_ms = 5i64;
    if until_target_ms > spin_threshold_ms {
        tokio::time::sleep(std::time::Duration::from_millis(
            (until_target_ms - spin_threshold_ms) as u64,
        ))
        .await;
    }

    loop {
        let current_epoch_ms = current_epoch_millis()?;
        if current_epoch_ms < *last_wall_epoch_ms {
            anyhow::bail!("System clock moved backwards; aborting");
        }
        if current_epoch_ms >= target_epoch_ms {
            *last_wall_epoch_ms = current_epoch_ms;
            break;
        }
        *last_wall_epoch_ms = current_epoch_ms;
        std::hint::spin_loop();
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
