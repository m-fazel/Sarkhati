use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_ntp_timeout_ms() -> u64 {
    1_000
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NtpConfig {
    pub server: String,
    #[serde(default = "default_ntp_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub ntp: Option<NtpConfig>,
}

pub fn load_config(path: &str) -> Result<Option<GlobalConfig>> {
    if !Path::new(path).exists() {
        return Ok(None);
    }
    let config_str =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path))?;
    let config: GlobalConfig =
        serde_json::from_str(&config_str).with_context(|| format!("Failed to parse {}", path))?;
    Ok(Some(config))
}
