//! DNS 探测器。
//!
//! 当前实现使用 Tokio 的系统解析器，先满足个人自部署的实用需求；
//! 后续可替换为 hickory-resolver 以支持更精细的记录类型和 DNS 服务器配置。

use std::time::{Duration, Instant};

use tokio::{net::lookup_host, time};

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
    probes::observation::{ProbeObservation, is_success},
};

/// 解析目标域名并记录耗时，可选校验期望 IP/值。
pub async fn probe(monitor: &Monitor, timeout: Duration) -> Result<CheckResult, AppError> {
    let started = Instant::now();
    let target = if monitor.target.contains(':') {
        monitor.target.clone()
    } else {
        format!("{}:80", monitor.target)
    };
    tracing::debug!(
        monitor_id = monitor.id,
        target = %target,
        timeout_ms = timeout.as_millis(),
        "starting dns probe"
    );

    let addrs = time::timeout(timeout, lookup_host(&target))
        .await
        .map_err(|_| AppError::BadRequest("dns lookup timed out".to_string()))?
        .map_err(anyhow::Error::from)?;
    let values: Vec<String> = addrs.map(|addr| addr.ip().to_string()).collect();
    let latency_us = started.elapsed().as_micros() as u64;
    let mut observation = ProbeObservation::new(latency_us);
    observation.dns_answers = values;
    let answer_count = observation.dns_answers.len();

    let success = is_success(monitor, &observation);
    tracing::debug!(
        monitor_id = monitor.id,
        answer_count = answer_count,
        latency_us = latency_us,
        success = success,
        "dns probe observed answers"
    );

    if success {
        Ok(CheckResult::success(monitor.id, latency_us))
    } else {
        Ok(CheckResult::failed(monitor.id, Some(latency_us)))
    }
}
