use anyhow::{Context, Result};
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue,
    ORIGIN, REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Debug, Deserialize, Clone)]
pub struct BidarConfig {
    pub authorization: String,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    #[serde(default)]
    pub x_user_trace: String,
    pub orders: Vec<BidarOrderData>,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<BidarCalibrationConfig>,
    #[serde(default = "default_rate_limit_ms")]
    pub rate_limit_ms: u64,
    #[serde(default)]
    pub delay_model: BidarDelayModel,
}

fn default_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0".to_string()
}

fn default_order_url() -> String {
    "https://api.bidartrader.ir/trader/v1/order/buy".to_string()
}

fn default_batch_delay() -> u64 {
    100
}

fn default_rate_limit_ms() -> u64 {
    300
}

fn default_calibration_enabled() -> bool {
    true
}

fn default_probe_count() -> usize {
    15
}

fn default_probe_interval_ms() -> u64 {
    300
}

fn default_warmup_probes() -> usize {
    3
}

fn default_safety_margin_ms() -> u64 {
    3
}

fn default_max_acceptable_rtt_ms() -> u64 {
    500
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BidarOrderData {
    #[serde(rename = "type")]
    pub order_type: String,
    pub quantity: String,
    pub isin: String,
    pub validity: String,
    pub price: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BidarDelayModel {
    Rtt,
    HalfRtt,
}

impl Default for BidarDelayModel {
    fn default() -> Self {
        Self::Rtt
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BidarCalibrationEstimator {
    P50,
    P75,
    P90,
    Min,
    Ewma,
}

impl Default for BidarCalibrationEstimator {
    fn default() -> Self {
        Self::P50
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct BidarCalibrationConfig {
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
    pub estimator: BidarCalibrationEstimator,
    #[serde(default = "default_max_acceptable_rtt_ms")]
    pub max_acceptable_rtt_ms: u64,
}

#[derive(Debug)]
pub struct CalibrationSummary {
    pub estimated_delay_ms: u64,
    pub last_probe_wall_time: SystemTime,
}

pub struct RateLimiter {
    rate_limit: Duration,
    last_request: Mutex<Option<Instant>>,
}

impl RateLimiter {
    pub fn new(rate_limit_ms: u64) -> Self {
        Self {
            rate_limit: Duration::from_millis(rate_limit_ms),
            last_request: Mutex::new(None),
        }
    }

    pub async fn wait(&self) {
        let mut last_request = self.last_request.lock().await;
        if let Some(last) = *last_request {
            let elapsed = last.elapsed();
            if elapsed < self.rate_limit {
                sleep(self.rate_limit - elapsed).await;
            }
        }
        *last_request = Some(Instant::now());
    }
}

pub async fn send_order(
    config: &BidarConfig,
    order: &BidarOrderData,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let order_json = serde_json::to_string(order)?;

    // Authorization header
    let auth_value = if config.authorization.starts_with("Bearer ") {
        config.authorization.clone()
    } else {
        format!("Bearer {}", config.authorization)
    };

    // Print curl command in test mode
    if test_mode {
        let x_user_trace_header = if !config.x_user_trace.is_empty() {
            format!("-H 'x-user-trace: {}' \\\n  ", config.x_user_trace)
        } else {
            String::new()
        };
        println!("[Bidar] Equivalent curl command:");
        println!(
            r#"curl '{}' \
  --compressed \
  -X POST \
  -H 'User-Agent: {}' \
  -H 'Accept: application/json' \
  -H 'Accept-Language: en-US,en;q=0.5' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'Content-Type: application/json' \
  -H 'Referer: https://bidartrader.ir/' \
  -H 'Origin: https://bidartrader.ir' \
  -H 'Authorization: {}' \
  {}-H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-site' \
  -H 'Connection: keep-alive' \
  -H 'Priority: u=0' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  -H 'TE: trailers' \
  --data-raw '{}'"#,
            config.order_url, config.user_agent, auth_value, x_user_trace_header, order_json
        );
        println!();

        // If curl_only, don't send the request
        if curl_only {
            return Ok(());
        }
    }

    if let Some(limiter) = rate_limiter {
        limiter.wait().await;
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.5"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://bidartrader.ir/"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://bidartrader.ir"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("empty"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("cors"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-site"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("Priority", HeaderValue::from_static("u=0"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("TE", HeaderValue::from_static("trailers"));

    headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);

    // x-user-trace header (optional)
    if !config.x_user_trace.is_empty() {
        headers.insert("x-user-trace", HeaderValue::from_str(&config.x_user_trace)?);
    }

    let body_bytes = order_json.as_bytes();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&body_bytes.len().to_string())?,
    );

    println!("[Bidar] Sending order JSON: {}", order_json);

    let response = client
        .post(&config.order_url)
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

    println!("[Bidar] Order response status: {}", status);
    println!("[Bidar] Order response body: {}", decoded_text);

    if !status.is_success() {
        anyhow::bail!("Order failed with status {}: {}", status, decoded_text);
    }

    Ok(())
}

pub async fn run_calibration(
    config: &BidarConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Result<CalibrationSummary> {
    let calibration = config
        .calibration
        .as_ref()
        .context("Calibration config missing")?;

    if calibration.probe_interval_ms < config.rate_limit_ms {
        anyhow::bail!(
            "probe_interval_ms ({}) must be >= rate_limit_ms ({})",
            calibration.probe_interval_ms,
            config.rate_limit_ms
        );
    }

    if calibration.warmup_probes >= calibration.probe_count {
        anyhow::bail!("warmup_probes must be less than probe_count");
    }

    let mut rtts_ms = Vec::with_capacity(calibration.probe_count);
    let mut last_probe_wall = SystemTime::now();
    let mut last_wall_time = SystemTime::now();

    println!(
        "[Bidar] Calibration enabled: {} probes every {}ms (warmup: {})",
        calibration.probe_count, calibration.probe_interval_ms, calibration.warmup_probes
    );

    for probe_index in 0..calibration.probe_count {
        let probe_start = Instant::now();

        rate_limiter.wait().await;
        let current_wall = SystemTime::now();
        if current_wall < last_wall_time {
            anyhow::bail!("System clock moved backwards during calibration; aborting");
        }
        last_wall_time = current_wall;
        let (rtt_ms, rtt_micros, status) = send_probe(client, config).await?;
        last_probe_wall = SystemTime::now();

        println!(
            "[Bidar] Probe #{}/{} status={} rtt={}ms ({}µs)",
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
        BidarCalibrationEstimator::P50 => p50_ms,
        BidarCalibrationEstimator::P75 => p75_ms,
        BidarCalibrationEstimator::P90 => p90_ms,
        BidarCalibrationEstimator::Min => min_ms,
        BidarCalibrationEstimator::Ewma => ewma(&samples_ms, 0.3),
    };

    println!(
        "[Bidar] Calibration stats: min={}ms p50={}ms p75={}ms p90={}ms max={}ms jitter={}ms estimator={:?} estimate={}ms",
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

async fn send_probe(
    client: &reqwest::Client,
    config: &BidarConfig,
) -> Result<(u64, u128, StatusCode)> {
    let t0 = Instant::now();

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(&config.user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));

    if !config.authorization.is_empty() {
        let auth_value = if config.authorization.starts_with("Bearer ") {
            config.authorization.clone()
        } else {
            format!("Bearer {}", config.authorization)
        };
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);
    }

    let base_url = probe_url(&config.order_url)?;
    let response = client.head(base_url).headers(headers).send().await?;
    let status = response.status();

    let rtt = t0.elapsed();
    let rtt_micros = rtt.as_micros();
    let rtt_ms = rtt.as_millis() as u64;

    Ok((rtt_ms, rtt_micros, status))
}

fn probe_url(order_url: &str) -> Result<String> {
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
    value.round() as u64
}
