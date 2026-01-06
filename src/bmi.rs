use crate::calibration::CalibrationConfig;
use crate::rate_limiter::RateLimiter;
use crate::standard_broker::{self, StandardBrokerConfig, StandardOrderData};
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct BmiConfig {
    pub cookie: String,
    #[serde(default = "standard_broker::default_user_agent")]
    pub user_agent: String,
    #[serde(default = "default_order_url")]
    pub order_url: String,
    pub orders: Vec<BmiOrderData>,
    #[serde(default = "standard_broker::default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default)]
    pub target_time: Option<String>,
    #[serde(default)]
    pub calibration: Option<CalibrationConfig>,
}

fn default_order_url() -> String {
    "https://api2.bmibourse.ir/Web/V1/Order/Post".to_string()
}

pub type BmiOrderData = StandardOrderData;

impl StandardBrokerConfig for BmiConfig {
    fn cookie(&self) -> &str {
        &self.cookie
    }

    fn user_agent(&self) -> &str {
        &self.user_agent
    }

    fn order_url(&self) -> &str {
        &self.order_url
    }

    fn calibration(&self) -> Option<&CalibrationConfig> {
        self.calibration.as_ref()
    }
}

pub async fn send_order(
    config: &BmiConfig,
    order: &BmiOrderData,
    test_mode: bool,
    curl_only: bool,
    rate_limiter: Option<&RateLimiter>,
) -> Result<()> {
    let order_json = serde_json::to_string(order)?;
    standard_broker::send_order(
        "BMI",
        "https://online.bmibourse.ir",
        "https://online.bmibourse.ir/",
        config,
        &order_json,
        test_mode,
        curl_only,
        rate_limiter,
    )
    .await
}

pub async fn run_calibration(
    config: &BmiConfig,
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Result<calibration::CalibrationSummary> {
    standard_broker::run_calibration("BMI", config, client, rate_limiter).await
}
