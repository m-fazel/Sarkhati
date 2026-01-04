use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct RateLimiter {
    rate_limit: Duration,
    last_request: Mutex<Option<Instant>>,
    rate_limit_ms: u64,
}

impl RateLimiter {
    pub fn new(rate_limit_ms: u64) -> Self {
        Self {
            rate_limit: Duration::from_millis(rate_limit_ms),
            last_request: Mutex::new(None),
            rate_limit_ms,
        }
    }

    pub fn rate_limit_ms(&self) -> u64 {
        self.rate_limit_ms
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
