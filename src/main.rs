use anyhow::{Context, Result};
use chrono::TimeZone;
use chrono_tz::Asia::Tehran;
use futures::future::join_all;
use std::env;
use std::fs;

mod bidar;
mod calibration;
mod danayan;
mod easy_trader;
mod exir_broker;
mod global_config;
mod logging;
mod mofid_online_plus;
mod online_plus_broker;
mod rate_limiter;
mod time_reference;

use crate::logging::{log_error, log_info, log_success, log_warn};

async fn run_parallel_scheduled_repeats<F, Fut>(
    label: &str,
    batch_repeat: usize,
    batch_delay_ms: u64,
    final_send_epoch_ms: i64,
    order_index: usize,
    total_orders: usize,
    mut send_fn: F,
) -> Result<()>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let mut tasks = Vec::with_capacity(batch_repeat);
    for repeat_index in 0..batch_repeat {
        let label = label.to_owned();
        let scheduled_epoch_ms = final_send_epoch_ms + repeat_index as i64 * batch_delay_ms as i64;
        let request_future = send_fn(repeat_index);
        tasks.push(async move {
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
            if scheduled_epoch_ms > now_epoch_ms {
                tokio::time::sleep(std::time::Duration::from_millis(
                    (scheduled_epoch_ms - now_epoch_ms) as u64,
                ))
                .await;
            }

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

            match request_future.await {
                Ok(_) => log_success(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }
            Ok::<(), anyhow::Error>(())
        });
    }

    for result in join_all(tasks).await {
        result?;
    }

    Ok(())
}

async fn run_parallel_continuous_cycle<F, Fut>(
    label: &str,
    batch_repeat: usize,
    batch_delay_ms: u64,
    order_index: usize,
    total_orders: usize,
    mut send_fn: F,
) -> Result<()>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let cycle_start = tokio::time::Instant::now();
    let mut tasks = Vec::with_capacity(batch_repeat);

    for repeat_index in 0..batch_repeat {
        let label = label.to_owned();
        let send_at =
            cycle_start + std::time::Duration::from_millis(repeat_index as u64 * batch_delay_ms);
        let request_future = send_fn(repeat_index);
        tasks.push(async move {
            tokio::time::sleep_until(send_at).await;
            match request_future.await {
                Ok(_) => log_success(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} sent",
                        repeat_index + 1,
                        batch_repeat,
                        order_index + 1,
                        total_orders
                    ),
                ),
                Err(e) => log_error(
                    &label,
                    &format!(
                        "Batch {}/{} Order {}/{} failed: {}",
                        repeat_index + 1,
                        batch_repeat,
                        order_index + 1,
                        total_orders,
                        e
                    ),
                ),
            }
            Ok::<(), anyhow::Error>(())
        });
    }

    for result in join_all(tasks).await {
        result?;
    }

    Ok(())
}

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
                    "Usage: {} <mofid_online_plus|danayan|bidar|online_plus|easy_trader|exir|all|BROKER_NAME> [test] [curl]",
                    args[0]
                ),
            );
            log_error(
                "CLI",
                "BROKER_NAME comes from config_online_plus.json, config_easy_trader.json, or config_exir.json.",
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
                    "Usage: {} <mofid_online_plus|danayan|bidar|online_plus|easy_trader|exir|all|BROKER_NAME> [test] [curl]",
                    args[0]
                ),
            );
            log_error(
                "CLI",
                "BROKER_NAME comes from config_online_plus.json, config_easy_trader.json, or config_exir.json.",
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
        "mofid_online_plus" => run_mofid_online_plus(test_mode, curl_only).await,
        "danayan" => run_danayan(test_mode, curl_only).await,
        "bidar" => run_bidar(test_mode, curl_only).await,
        "online_plus" => run_online_plus(test_mode, curl_only).await,
        "easy_trader" => run_easy_trader(test_mode, curl_only).await,
        "exir" => run_exir(test_mode, curl_only).await,
        "all" => run_all(test_mode, curl_only).await,
        other => match run_online_plus_broker_by_name(other, test_mode, curl_only).await {
            Ok(()) => Ok(()),
            Err(_) => match run_easy_trader_broker_by_name(other, test_mode, curl_only).await {
                Ok(()) => Ok(()),
                Err(_) => run_exir_broker_by_name(other, test_mode, curl_only).await,
            },
        },
    }
}

async fn run_all(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("System", "Starting Sarkhati - All Brokers in Parallel");

    let standard_config = online_plus_broker::load_config("config_online_plus.json")?;
    let easy_trader_config = easy_trader::load_config("config_easy_trader.json")?;
    let exir_config = exir_broker::load_config("config_exir.json")?;

    let mofid_handle = tokio::spawn(async move {
        if let Err(e) = run_mofid_online_plus(test_mode, curl_only).await {
            log_error("MofidOnlinePlus", &format!("Stopped with error: {}", e));
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
            if let Err(e) = run_online_plus_broker(broker, test_mode, curl_only).await {
                log_error("OnlinePlus", &format!("Stopped with error: {}", e));
            }
        });
        standard_handles.push(handle);
    }

    let mut easy_trader_handles = Vec::new();
    for broker in easy_trader_config.accounts.clone() {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_easy_trader_broker(broker, test_mode, curl_only).await {
                log_error("EasyTrader", &format!("Stopped with error: {}", e));
            }
        });
        easy_trader_handles.push(handle);
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
    for handle in easy_trader_handles {
        let _ = handle.await;
    }
    for handle in exir_handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_online_plus(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("OnlinePlus", "Starting Sarkhati - Online Plus Brokers");

    let standard_config = online_plus_broker::load_config("config_online_plus.json")?;
    if standard_config.accounts.is_empty() {
        log_warn(
            "OnlinePlus",
            "No accounts found in config_online_plus.json; skipping.",
        );
        return Ok(());
    }

    let mut handles = Vec::new();
    for broker in standard_config.accounts {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_online_plus_broker(broker, test_mode, curl_only).await {
                log_error("OnlinePlus", &format!("Stopped with error: {}", e));
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

async fn run_easy_trader(test_mode: bool, curl_only: bool) -> Result<()> {
    log_info("EasyTrader", "Starting Sarkhati - Easy Trader Brokers");

    let easy_trader_config = easy_trader::load_config("config_easy_trader.json")?;
    if easy_trader_config.accounts.is_empty() {
        log_warn(
            "EasyTrader",
            "No accounts found in config_easy_trader.json; skipping.",
        );
        return Ok(());
    }

    let mut handles = Vec::new();
    for broker in easy_trader_config.accounts {
        let handle = tokio::spawn(async move {
            if let Err(e) = run_easy_trader_broker(broker, test_mode, curl_only).await {
                log_error("EasyTrader", &format!("Stopped with error: {}", e));
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn run_online_plus_broker_by_name(
    name: &str,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let config = online_plus_broker::load_config("config_online_plus.json")?;
    let broker = online_plus_broker::find_broker(&config, name)
        .cloned()
        .with_context(|| format!("Broker '{}' not found in config_online_plus.json", name))?;
    run_online_plus_broker(broker, test_mode, curl_only).await
}

async fn run_exir_broker_by_name(name: &str, test_mode: bool, curl_only: bool) -> Result<()> {
    let config = exir_broker::load_config("config_exir.json")?;
    let broker = exir_broker::find_broker(&config, name)
        .cloned()
        .with_context(|| format!("Broker '{}' not found in config_exir.json", name))?;
    run_exir_broker(broker, test_mode, curl_only).await
}

async fn run_easy_trader_broker_by_name(
    name: &str,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let config = easy_trader::load_config("config_easy_trader.json")?;
    let broker = easy_trader::find_broker(&config, name)
        .cloned()
        .with_context(|| format!("Broker '{}' not found in config_easy_trader.json", name))?;
    run_easy_trader_broker(broker, test_mode, curl_only).await
}

async fn run_online_plus_broker(
    broker: online_plus_broker::OnlinePlusBrokerConfig,
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
            "Cookie is required for {}. Please set 'cookie' in config_online_plus.json",
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
        online_plus_broker::send_order(
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
                if let Err(e) = run_online_plus_order_scheduled_task(
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
            if let Err(e) = run_online_plus_order_continuous(
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

async fn run_easy_trader_broker(
    broker: easy_trader::EasyTraderBrokerConfig,
    test_mode: bool,
    curl_only: bool,
) -> Result<()> {
    let rate_limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));

    log_info(
        &broker.name,
        &format!("Starting Sarkhati - {} Order Sender", broker.name),
    );

    if broker.authorization.is_empty() {
        anyhow::bail!(
            "Authorization token is required for {}. Please set 'authorization' in config_easy_trader.json",
            broker.name
        );
    }

    let token = broker
        .authorization
        .strip_prefix("Bearer ")
        .unwrap_or(&broker.authorization);

    log_info(
        &broker.name,
        &format!(
            "Bearer auth enabled (preview: {}...)",
            &token[..token.len().min(50)]
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
        easy_trader::send_order(
            &broker,
            order,
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
            let order_clone = order.clone();
            let broker_name = broker.name.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = run_easy_trader_order_scheduled_task(
                    broker_clone,
                    order_clone,
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
        let order_clone = order.clone();
        let broker_name = broker.name.clone();
        let limiter = std::sync::Arc::new(rate_limiter::RateLimiter::new(broker.batch_delay_ms));
        let handle = tokio::spawn(async move {
            if let Err(e) = run_easy_trader_order_continuous(
                broker_clone,
                order_clone,
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

async fn run_mofid_online_plus_account(
    config: mofid_online_plus::MofidOnlinePlusConfig,
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
        mofid_online_plus::send_order(
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

async fn run_mofid_online_plus(test_mode: bool, curl_only: bool) -> Result<()> {
    let config_str = fs::read_to_string("config_mofid_online_plus.json")
        .context("Failed to read config_mofid_online_plus.json")?;
    let config_file: mofid_online_plus::MofidOnlinePlusConfigFile =
        serde_json::from_str(&config_str)
            .context("Failed to parse config_mofid_online_plus.json")?;
    let accounts = match config_file {
        mofid_online_plus::MofidOnlinePlusConfigFile::Single(config) => vec![config],
        mofid_online_plus::MofidOnlinePlusConfigFile::Multiple { accounts } => accounts,
        mofid_online_plus::MofidOnlinePlusConfigFile::List(accounts) => accounts,
    };

    if accounts.is_empty() {
        log_warn(
            "MofidOnlinePlus",
            "No accounts found in config_mofid_online_plus.json; skipping.",
        );
        return Ok(());
    }

    let mut handles = Vec::new();
    for (index, mut account) in accounts.into_iter().enumerate() {
        let fallback_label = format!("MofidOnlinePlus#{}", index + 1);
        let label = account.display_name(&fallback_label);
        if account.name.is_none() {
            account.name = Some(label.clone());
        }
        let handle = tokio::spawn(async move {
            if let Err(e) =
                run_mofid_online_plus_account(account, label.clone(), test_mode, curl_only).await
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

async fn run_online_plus_order_schedule(
    broker: online_plus_broker::OnlinePlusBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    run_parallel_scheduled_repeats(
        &broker.name,
        broker.batch_repeat,
        broker.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let broker = broker.clone();
            let order_json = order_json.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                online_plus_broker::send_order(
                    &broker,
                    &order_json,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
}

async fn run_online_plus_order_scheduled_task(
    broker: online_plus_broker::OnlinePlusBrokerConfig,
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
            let summary = online_plus_broker::run_calibration(
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

        run_online_plus_order_schedule(
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

async fn run_online_plus_order_continuous(
    broker: online_plus_broker::OnlinePlusBrokerConfig,
    order_json: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        run_parallel_continuous_cycle(
            &broker.name,
            broker.batch_repeat,
            broker.batch_delay_ms,
            order_index,
            total_orders,
            |repeat_index| {
                let broker = broker.clone();
                let order_json = order_json.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    let _ = repeat_index;
                    online_plus_broker::send_order(
                        &broker,
                        &order_json,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&broker.name, "Test mode: exiting after one batch cycle");
            return Ok(());
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
    run_parallel_scheduled_repeats(
        &broker.name,
        broker.batch_repeat,
        broker.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let broker = broker.clone();
            let order_json = order_json.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                exir_broker::send_order(
                    &broker,
                    &order_json,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
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
        run_parallel_continuous_cycle(
            &broker.name,
            broker.batch_repeat,
            broker.batch_delay_ms,
            order_index,
            total_orders,
            |_repeat_index| {
                let broker = broker.clone();
                let order_json = order_json.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    exir_broker::send_order(
                        &broker,
                        &order_json,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&broker.name, "Test mode: exiting after one batch cycle");
            return Ok(());
        }
    }
}

async fn run_easy_trader_order_schedule(
    broker: easy_trader::EasyTraderBrokerConfig,
    order: easy_trader::EasyTraderOrderData,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    run_parallel_scheduled_repeats(
        &broker.name,
        broker.batch_repeat,
        broker.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let broker = broker.clone();
            let order = order.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                easy_trader::send_order(
                    &broker,
                    &order,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
}

async fn run_easy_trader_order_scheduled_task(
    broker: easy_trader::EasyTraderBrokerConfig,
    order: easy_trader::EasyTraderOrderData,
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
            let summary = easy_trader::run_calibration(
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

        run_easy_trader_order_schedule(
            broker.clone(),
            order.clone(),
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

async fn run_easy_trader_order_continuous(
    broker: easy_trader::EasyTraderBrokerConfig,
    order: easy_trader::EasyTraderOrderData,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        run_parallel_continuous_cycle(
            &broker.name,
            broker.batch_repeat,
            broker.batch_delay_ms,
            order_index,
            total_orders,
            |_repeat_index| {
                let broker = broker.clone();
                let order = order.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    easy_trader::send_order(
                        &broker,
                        &order,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&broker.name, "Test mode: exiting after one batch cycle");
            return Ok(());
        }
    }
}

async fn run_mofid_order_schedule(
    config: mofid_online_plus::MofidOnlinePlusConfig,
    order: mofid_online_plus::MofidOnlinePlusOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    final_send_epoch_ms: i64,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    run_parallel_scheduled_repeats(
        &label,
        config.batch_repeat,
        config.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let config = config.clone();
            let order = order.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                mofid_online_plus::send_order(
                    &config,
                    &order,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
}

async fn run_mofid_order_scheduled_task(
    config: mofid_online_plus::MofidOnlinePlusConfig,
    order: mofid_online_plus::MofidOnlinePlusOrderData,
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
            let summary = mofid_online_plus::run_calibration(
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
    config: mofid_online_plus::MofidOnlinePlusConfig,
    order: mofid_online_plus::MofidOnlinePlusOrderData,
    label: String,
    order_index: usize,
    total_orders: usize,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: std::sync::Arc<rate_limiter::RateLimiter>,
) -> Result<()> {
    loop {
        run_parallel_continuous_cycle(
            &label,
            config.batch_repeat,
            config.batch_delay_ms,
            order_index,
            total_orders,
            |_repeat_index| {
                let config = config.clone();
                let order = order.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    mofid_online_plus::send_order(
                        &config,
                        &order,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&label, "Test mode: exiting after one batch cycle");
            return Ok(());
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
    run_parallel_scheduled_repeats(
        &label,
        config.batch_repeat,
        config.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let config = config.clone();
            let order = order.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                danayan::send_order(
                    &config,
                    &order,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
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
        run_parallel_continuous_cycle(
            &label,
            config.batch_repeat,
            config.batch_delay_ms,
            order_index,
            total_orders,
            |_repeat_index| {
                let config = config.clone();
                let order = order.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    danayan::send_order(
                        &config,
                        &order,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&label, "Test mode: exiting after one batch cycle");
            return Ok(());
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
    run_parallel_scheduled_repeats(
        &label,
        config.batch_repeat,
        config.batch_delay_ms,
        final_send_epoch_ms,
        order_index,
        total_orders,
        |_repeat_index| {
            let config = config.clone();
            let order = order.clone();
            let rate_limiter = rate_limiter.clone();
            async move {
                bidar::send_order(
                    &config,
                    &order,
                    test_mode,
                    curl_only,
                    Some(rate_limiter.as_ref()),
                )
                .await
            }
        },
    )
    .await
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
        run_parallel_continuous_cycle(
            &label,
            config.batch_repeat,
            config.batch_delay_ms,
            order_index,
            total_orders,
            |_repeat_index| {
                let config = config.clone();
                let order = order.clone();
                let rate_limiter = rate_limiter.clone();
                async move {
                    bidar::send_order(
                        &config,
                        &order,
                        test_mode,
                        curl_only,
                        Some(rate_limiter.as_ref()),
                    )
                    .await
                }
            },
        )
        .await?;

        if test_mode {
            log_info(&label, "Test mode: exiting after one batch cycle");
            return Ok(());
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
