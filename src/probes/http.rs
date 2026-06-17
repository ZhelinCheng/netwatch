//! HTTP/HTTPS 探测器。

use std::time::{Duration, Instant};

use reqwest::Client;

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
    probes::observation::{ProbeObservation, is_success},
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
    let headers = response
        .headers()
        .iter()
        .filter_map(|(key, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (key.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect();
    let should_read_body = monitor.config.keyword.is_some()
        || monitor
            .config
            .success_rules
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|rule| matches!(rule, crate::domain::monitor::SuccessRule::HttpBody { .. }));
    let body = if should_read_body {
        Some(response.text().await.map_err(anyhow::Error::from)?)
    } else {
        None
    };
    let latency_us = started.elapsed().as_micros() as u64;
    let mut observation = ProbeObservation::new(latency_us);
    observation.http_status = Some(status);
    observation.http_headers = headers;
    observation.http_body = body;

    if is_success(monitor, &observation) {
        Ok(CheckResult::success(monitor.id.clone(), latency_us))
    } else {
        Ok(CheckResult::failed(monitor.id.clone(), Some(latency_us)))
    }
}
