//! Ping 探测器。
//!
//! 为了避免 raw socket/root 权限问题，第一版调用系统 `ping` 命令完成 ICMP 探测。

use std::{process::Stdio, time::Instant};

use tokio::{process::Command, time};

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
};

pub async fn probe(
    monitor: &Monitor,
    timeout: std::time::Duration,
) -> Result<CheckResult, AppError> {
    let started = Instant::now();
    let output = time::timeout(
        timeout,
        Command::new("ping")
            .arg("-c")
            .arg("1")
            .arg(&monitor.target)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| AppError::BadRequest("ping timed out".to_string()))?
    .map_err(anyhow::Error::from)?;

    let latency_us = started.elapsed().as_micros() as u64;

    if output.status.success() {
        Ok(CheckResult::success(monitor.id.clone(), latency_us))
    } else {
        Ok(CheckResult::failed(monitor.id.clone(), Some(latency_us)))
    }
}
