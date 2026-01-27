use crate::logging::{log_info, log_warn};
use crate::rate_limiter::RateLimiter;
use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::{Duration, Instant, SystemTime};
use tokio::time::sleep;

fn default_calibration_enabled() -> bool {
    true
}

fn default_probe_count() -> usize {
    10
}

fn default_probe_interval_ms() -> u64 {
    300
}

fn default_warmup_probes() -> usize {
    2
}

fn default_safety_margin_ms() -> u64 {
    0
}

fn default_max_acceptable_rtt_ms() -> u64 {
    500
}

fn default_calibration_window_ms() -> u64 {
    10_000
}

fn default_calibration_start_margin_ms() -> u64 {
    1_000
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationEstimator {
    P50,
    P75,
    P90,
    Min,
    Ewma,
    WeightedRecent,
}

impl Default for CalibrationEstimator {
    fn default() -> Self {
        CalibrationEstimator::WeightedRecent
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CalibrationConfig {
    #[serde(default = "default_calibration_enabled")]
    pub enabled: bool,
    #[serde(default = "default_probe_count")]
    pub probe_count: usize,
    #[serde(default = "default_probe_interval_ms")]
    pub probe_interval_ms: u64,
    #[serde(default = "default_warmup_probes")]
    pub warmup_probes: usize,
    #[serde(default = "default_safety_margin_ms")]
    pub safety_margin_ms: u64,
    #[serde(default)]
    pub estimator: CalibrationEstimator,
    #[serde(default = "default_max_acceptable_rtt_ms")]
    pub max_acceptable_rtt_ms: u64,
    #[serde(default = "default_calibration_window_ms")]
    pub calibration_window_ms: u64,
    #[serde(default = "default_calibration_start_margin_ms")]
    pub calibration_start_margin_ms: u64,
}

#[derive(Debug)]
pub struct CalibrationSummary {
    pub estimated_delay_ms: u64,
    pub last_probe_wall_time: SystemTime,
}

pub async fn run_calibration<F, Fut>(
    broker_label: &str,
    calibration: &CalibrationConfig,
    rate_limiter: &RateLimiter,
    deadline_epoch_ms: Option<i64>,
    mut send_probe: F,
) -> Result<CalibrationSummary>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(u64, u128, StatusCode)>>,
{
    if calibration.probe_interval_ms < rate_limiter.rate_limit_ms() {
        log_warn(
            broker_label,
            &format!(
                "probe_interval_ms ({}) is less than batch_delay_ms ({}); continuing anyway",
                calibration.probe_interval_ms,
                rate_limiter.rate_limit_ms()
            ),
        );
    }

    let warmup_probes = calibration.warmup_probes;
    let max_probes = match deadline_epoch_ms {
        Some(_) => usize::MAX,
        None => calibration.probe_count.max(1),
    };

    let mut rtts_ms = Vec::with_capacity(calibration.probe_count.max(1));
    let mut last_probe_wall = crate::time_reference::now_system_time();
    let mut last_wall_time = crate::time_reference::now_system_time();

    log_info(
        broker_label,
        &format!(
            "Calibration enabled: probing every {}ms (warmup: {})",
            calibration.probe_interval_ms, warmup_probes
        ),
    );

    for probe_index in 0..max_probes {
        if let Some(deadline_epoch_ms) = deadline_epoch_ms {
            if crate::time_reference::now_system_time()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64 >= deadline_epoch_ms)
                .unwrap_or(true)
            {
                log_warn(
                    broker_label,
                    "Calibration deadline reached; finishing with collected samples",
                );
                break;
            }
        }

        let probe_start = Instant::now();

        rate_limiter.wait().await;
        if let Some(deadline_epoch_ms) = deadline_epoch_ms {
            if crate::time_reference::now_system_time()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as i64 >= deadline_epoch_ms)
                .unwrap_or(true)
            {
                log_warn(
                    broker_label,
                    "Calibration deadline reached before probe send; finishing early",
                );
                break;
            }
        }
        let current_wall = crate::time_reference::now_system_time();
        if current_wall < last_wall_time {
            log_warn(
                broker_label,
                "System clock moved backwards during calibration; finishing early",
            );
            break;
        }
        last_wall_time = current_wall;
        let probe_result = send_probe().await;
        let (rtt_ms, rtt_micros, status) = match probe_result {
            Ok(values) => values,
            Err(err) => {
                log_warn(
                    broker_label,
                    &format!("Probe failed (index {}): {}", probe_index + 1, err),
                );
                if probe_index + 1 < calibration.probe_count {
                    let elapsed = probe_start.elapsed();
                    let target = Duration::from_millis(calibration.probe_interval_ms);
                    if elapsed < target {
                        sleep(target - elapsed).await;
                    }
                }
                continue;
            }
        };
        last_probe_wall = crate::time_reference::now_system_time();

        log_info(
            broker_label,
            &format!(
                "Probe #{} status={} rtt={}ms ({}µs)",
                probe_index + 1,
                status,
                rtt_ms,
                rtt_micros
            ),
        );

        if rtt_ms > calibration.max_acceptable_rtt_ms {
            log_warn(
                broker_label,
                &format!(
                    "Probe RTT {}ms exceeded max_acceptable_rtt_ms {}; marking as outlier",
                    rtt_ms, calibration.max_acceptable_rtt_ms
                ),
            );
        } else {
            rtts_ms.push(rtt_ms);
        }

        if probe_index + 1 < max_probes {
            let elapsed = probe_start.elapsed();
            let target = Duration::from_millis(calibration.probe_interval_ms);
            if elapsed < target {
                sleep(target - elapsed).await;
            }
        }
    }

    let mut samples_ms = rtts_ms
        .iter()
        .skip(warmup_probes)
        .copied()
        .collect::<Vec<_>>();

    if samples_ms.is_empty() {
        if !rtts_ms.is_empty() {
            log_warn(
                broker_label,
                "No calibration samples after warmup; using all collected samples",
            );
            samples_ms = rtts_ms.clone();
        } else {
            log_warn(
                broker_label,
                "No calibration samples available; using zero delay estimate",
            );
            return Ok(CalibrationSummary {
                estimated_delay_ms: 0,
                last_probe_wall_time: last_probe_wall,
            });
        }
    }

    let mut sorted = samples_ms.clone();
    sorted.sort_unstable();

    let min_ms = *sorted.first().unwrap();
    let max_ms = *sorted.last().unwrap();
    let p50_ms = percentile(&sorted, 50.0);
    let p75_ms = percentile(&sorted, 75.0);
    let p90_ms = percentile(&sorted, 90.0);
    let jitter_ms = p90_ms.saturating_sub(p50_ms);

    let estimated_delay_ms = match calibration.estimator {
        CalibrationEstimator::P50 => p50_ms,
        CalibrationEstimator::P75 => p75_ms,
        CalibrationEstimator::P90 => p90_ms,
        CalibrationEstimator::Min => min_ms,
        CalibrationEstimator::Ewma => ewma(&samples_ms, 0.3),
        CalibrationEstimator::WeightedRecent => weighted_recent(&samples_ms),
    };

    log_info(
        broker_label,
        &format!(
            "Calibration stats: min={}ms p50={}ms p75={}ms p90={}ms max={}ms jitter={}ms estimator={:?} estimate={}ms",
            min_ms,
            p50_ms,
            p75_ms,
            p90_ms,
            max_ms,
            jitter_ms,
            calibration.estimator,
            estimated_delay_ms
        ),
    );

    Ok(CalibrationSummary {
        estimated_delay_ms,
        last_probe_wall_time: last_probe_wall,
    })
}

pub fn probe_url(order_url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(order_url)
        .with_context(|| format!("Invalid order_url {}", order_url))?;
    let host = parsed.host_str().context("order_url missing host")?;
    let mut base = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        base.push_str(&format!(":{}", port));
    }
    Ok(base)
}

fn percentile(sorted: &[u64], percentile: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((percentile / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn ewma(samples: &[u64], alpha: f64) -> u64 {
    let mut iter = samples.iter();
    let Some(&first) = iter.next() else {
        return 0;
    };
    let mut value = first as f64;
    for &sample in iter {
        value = alpha * sample as f64 + (1.0 - alpha) * value;
    }
    value.round().max(0.0) as u64
}

fn weighted_recent(samples: &[u64]) -> u64 {
    let mut weighted_sum: u128 = 0;
    let mut weight_total: u128 = 0;
    for (index, &sample) in samples.iter().enumerate() {
        let weight = (index as u128) + 1;
        weighted_sum = weighted_sum.saturating_add(sample as u128 * weight);
        weight_total = weight_total.saturating_add(weight);
    }
    if weight_total == 0 {
        return 0;
    }
    (weighted_sum / weight_total) as u64
}
