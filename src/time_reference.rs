use crate::global_config::NtpConfig;
use crate::logging::{log_info, log_warn};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

const NTP_UNIX_EPOCH_DIFF: u64 = 2_208_988_800;

static CLOCK_OFFSET_MS: OnceLock<i64> = OnceLock::new();

pub async fn initialize(ntp: Option<&NtpConfig>) -> Result<()> {
    if let Some(ntp) = ntp {
        match query_ntp_offset_ms(&ntp.server, ntp.timeout_ms).await {
            Ok(offset_ms) => {
                if CLOCK_OFFSET_MS.set(offset_ms).is_err() {
                    log_warn(
                        "Time",
                        "Clock offset already initialized; keeping existing value.",
                    );
                } else {
                    log_info(
                        "Time",
                        &format!(
                            "NTP sync complete: server={} offset={}ms",
                            ntp.server, offset_ms
                        ),
                    );
                }
            }
            Err(err) => {
                log_warn(
                    "Time",
                    &format!(
                        "NTP sync failed for server={}: {}. Falling back to local system clock.",
                        ntp.server, err
                    ),
                );
                let _ = CLOCK_OFFSET_MS.set(0);
            }
        }
    }
    Ok(())
}

pub fn now_system_time() -> SystemTime {
    let offset_ms = *CLOCK_OFFSET_MS.get_or_init(|| 0);
    if offset_ms >= 0 {
        SystemTime::now() + Duration::from_millis(offset_ms as u64)
    } else {
        SystemTime::now() - Duration::from_millis((-offset_ms) as u64)
    }
}

pub fn now_utc() -> DateTime<Utc> {
    DateTime::<Utc>::from(now_system_time())
}

async fn query_ntp_offset_ms(server: &str, timeout_ms: u64) -> Result<i64> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("Failed to bind UDP socket")?;
    let addr = format!("{}:123", server);
    socket
        .connect(&addr)
        .await
        .with_context(|| format!("Failed to connect to NTP server {}", addr))?;

    let mut request = [0u8; 48];
    request[0] = 0b00_100_011;

    let t1 = SystemTime::now();
    write_transmit_timestamp(&mut request[40..48], t1)?;
    socket
        .send(&request)
        .await
        .context("Failed to send NTP request")?;

    let mut response = [0u8; 48];
    let recv_len = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        socket.recv(&mut response),
    )
    .await
    .context("NTP request timed out")?
    .context("Failed to receive NTP response")?;
    if recv_len < 48 {
        anyhow::bail!("Incomplete NTP response ({} bytes)", recv_len);
    }
    let t4 = SystemTime::now();

    let t2 = read_timestamp_nanos(&response[32..40])?;
    let t3 = read_timestamp_nanos(&response[40..48])?;
    let t1_nanos = system_time_to_ntp_nanos(t1)?;
    let t4_nanos = system_time_to_ntp_nanos(t4)?;

    let offset_nanos = ((t2 - t1_nanos) + (t3 - t4_nanos)) / 2;
    Ok((offset_nanos / 1_000_000) as i64)
}

fn system_time_to_ntp_nanos(time: SystemTime) -> Result<i128> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .context("System time is before UNIX_EPOCH")?;
    let secs = duration.as_secs() + NTP_UNIX_EPOCH_DIFF;
    let nanos = duration.subsec_nanos() as u128;
    Ok(secs as i128 * 1_000_000_000 + nanos as i128)
}

fn write_transmit_timestamp(buf: &mut [u8], time: SystemTime) -> Result<()> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .context("System time is before UNIX_EPOCH")?;
    let secs = duration.as_secs() + NTP_UNIX_EPOCH_DIFF;
    let nanos = duration.subsec_nanos() as u128;
    let fraction = ((nanos << 32) / 1_000_000_000u128) as u32;
    buf[..4].copy_from_slice(&(secs as u32).to_be_bytes());
    buf[4..8].copy_from_slice(&fraction.to_be_bytes());
    Ok(())
}

fn read_timestamp_nanos(buf: &[u8]) -> Result<i128> {
    if buf.len() < 8 {
        anyhow::bail!("NTP timestamp buffer too small");
    }
    let secs = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as u128;
    let fraction = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as u128;
    if secs == 0 && fraction == 0 {
        anyhow::bail!("NTP timestamp missing in response");
    }
    let fraction_nanos = (fraction * 1_000_000_000u128) >> 32;
    Ok((secs * 1_000_000_000u128 + fraction_nanos) as i128)
}
