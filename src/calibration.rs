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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationEstimator {
    P50,
    P75,
    P90,
    Min,
    Ewma,
}

impl Default for CalibrationEstimator {
    fn default() -> Self {
        CalibrationEstimator::P50
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
    mut send_probe: F,
) -> Result<CalibrationSummary>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(u64, u128, StatusCode)>>,
{
    if calibration.probe_interval_ms < rate_limiter.rate_limit_ms() {
        anyhow::bail!(
            "probe_interval_ms ({}) must be >= batch_delay_ms ({})",
            calibration.probe_interval_ms,
            rate_limiter.rate_limit_ms()
        );
    }

    if calibration.warmup_probes >= calibration.probe_count {
        anyhow::bail!("warmup_probes must be less than probe_count");
    }

    let mut rtts_ms = Vec::with_capacity(calibration.probe_count);
    let mut last_probe_wall = SystemTime::now();
    let mut last_wall_time = SystemTime::now();

    println!(
        "{} Calibration enabled: {} probes every {}ms (warmup: {})",
        broker_label,
        calibration.probe_count,
        calibration.probe_interval_ms,
        calibration.warmup_probes
    );

    for probe_index in 0..calibration.probe_count {
        let probe_start = Instant::now();

        rate_limiter.wait().await;
        let current_wall = SystemTime::now();
        if current_wall < last_wall_time {
            anyhow::bail!("System clock moved backwards during calibration; aborting");
        }
        last_wall_time = current_wall;
        let (rtt_ms, rtt_micros, status) = send_probe().await?;
        last_probe_wall = SystemTime::now();

        println!(
            "{} Probe #{}/{} status={} rtt={}ms ({}µs)",
            broker_label,
            probe_index + 1,
            calibration.probe_count,
            status,
            rtt_ms,
            rtt_micros
        );

        if rtt_ms > calibration.max_acceptable_rtt_ms {
            anyhow::bail!(
                "Probe RTT {}ms exceeded max_acceptable_rtt_ms {}",
                rtt_ms,
                calibration.max_acceptable_rtt_ms
            );
        }

        rtts_ms.push(rtt_ms);

        if probe_index + 1 < calibration.probe_count {
            let elapsed = probe_start.elapsed();
            let target = Duration::from_millis(calibration.probe_interval_ms);
            if elapsed < target {
                sleep(target - elapsed).await;
            }
        }
    }

    let samples_ms = rtts_ms
        .iter()
        .skip(calibration.warmup_probes)
        .copied()
        .collect::<Vec<_>>();

    if samples_ms.is_empty() {
        anyhow::bail!("No calibration samples available after warmup.");
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
    };

    println!(
        "{} Calibration stats: min={}ms p50={}ms p75={}ms p90={}ms max={}ms jitter={}ms estimator={:?} estimate={}ms",
        broker_label,
        min_ms,
        p50_ms,
        p75_ms,
        p90_ms,
        max_ms,
        jitter_ms,
        calibration.estimator,
        estimated_delay_ms
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
