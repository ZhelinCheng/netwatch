//! DNS 探测器。
//!
//! 当前实现使用 Tokio 的系统解析器，先满足个人自部署的实用需求；
//! 后续可替换为 hickory-resolver 以支持更精细的记录类型和 DNS 服务器配置。

use std::time::{Duration, Instant};

use serde_json::json;
use tokio::{net::lookup_host, time};

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
};

/// 解析目标域名并记录耗时，可选校验期望 IP/值。
pub async fn probe(monitor: &Monitor, timeout: Duration) -> Result<CheckResult, AppError> {
    let started = Instant::now();
    let target = if monitor.target.contains(':') {
        monitor.target.clone()
    } else {
        format!("{}:80", monitor.target)
    };

    let addrs = time::timeout(timeout, lookup_host(&target))
        .await
        .map_err(|_| AppError::BadRequest("dns lookup timed out".to_string()))?
        .map_err(anyhow::Error::from)?;
    let values: Vec<String> = addrs.map(|addr| addr.ip().to_string()).collect();
    let latency_ms = started.elapsed().as_millis() as u64;

    let metadata = json!({
        "target": monitor.target,
        "record": monitor.config.dns_record.as_deref().unwrap_or("A"),
        "values": values
    });

    if let Some(expected) = &monitor.config.expected_value {
        if !values.iter().any(|value| value == expected) {
            return Ok(CheckResult::down(
                monitor.id.clone(),
                format!("expected DNS value not found: {expected}"),
                metadata,
            ));
        }
    }

    if values.is_empty() {
        return Ok(CheckResult::down(
            monitor.id.clone(),
            "dns lookup returned no records",
            metadata,
        ));
    }

    Ok(CheckResult::up(monitor.id.clone(), latency_ms, metadata))
}
