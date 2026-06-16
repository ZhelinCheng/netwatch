//! HTTP/HTTPS 探测器。

use std::time::{Duration, Instant};

use reqwest::Client;
use serde_json::json;

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
};

/// 检查目标 URL 的状态码，并可选检查响应体关键字。
pub async fn probe(monitor: &Monitor, timeout: Duration) -> Result<CheckResult, AppError> {
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(anyhow::Error::from)?;
    let started = Instant::now();
    let response = client
        .get(&monitor.target)
        .send()
        .await
        .map_err(anyhow::Error::from)?;

    let status = response.status().as_u16();
    let expected = monitor.config.expected_status.unwrap_or(200);
    // 只有配置了关键字时才读取响应体，避免普通状态码检查产生额外开销。
    let body = if monitor.config.keyword.is_some() {
        Some(response.text().await.map_err(anyhow::Error::from)?)
    } else {
        None
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    let metadata = json!({
        "target": monitor.target,
        "status": status,
        "expected_status": expected
    });

    if status != expected {
        return Ok(CheckResult::down(
            monitor.id.clone(),
            format!("expected status {expected}, got {status}"),
            metadata,
        ));
    }

    if let Some(keyword) = &monitor.config.keyword {
        let body = body.unwrap_or_default();
        if !body.contains(keyword) {
            return Ok(CheckResult::down(
                monitor.id.clone(),
                format!("keyword not found: {keyword}"),
                metadata,
            ));
        }
    }

    Ok(CheckResult::up(monitor.id.clone(), latency_ms, metadata))
}
