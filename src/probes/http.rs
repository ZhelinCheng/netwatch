//! HTTP/HTTPS 探测器。

use std::time::{Duration, Instant};

use reqwest::Client;

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
    probes::observation::{ProbeObservation, is_success},
};

/// 检查目标 URL 的状态码，并可选检查响应体正则。
pub async fn probe(monitor: &Monitor, timeout: Duration) -> Result<CheckResult, AppError> {
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(anyhow::Error::from)?;
    let started = Instant::now();
    tracing::debug!(
        monitor_id = monitor.id,
        target = %monitor.target,
        timeout_ms = timeout.as_millis(),
        "starting http probe"
    );
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
    let should_read_body = should_read_body(monitor);
    let body = if should_read_body {
        Some(response.text().await.map_err(anyhow::Error::from)?)
    } else {
        None
    };
    let latency_us = started.elapsed().as_micros() as u64;
    let mut observation = ProbeObservation::new();
    observation.http_status = Some(status);
    observation.http_headers = headers;
    observation.http_body = body;

    let success = is_success(monitor, &observation);
    tracing::debug!(
        monitor_id = monitor.id,
        status = status,
        latency_us = latency_us,
        success = success,
        "http probe observed response"
    );

    if success {
        Ok(CheckResult::success(monitor.id, latency_us))
    } else {
        Ok(CheckResult::failed(monitor.id, Some(latency_us)))
    }
}

/// 只有关键字正则存在时才读取 body，避免无意义地加载大响应。
fn should_read_body(monitor: &Monitor) -> bool {
    monitor.config.keyword.is_some()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::monitor::{HttpHeaderMatch, MonitorConfig, MonitorKind};

    use super::*;

    #[test]
    fn http_rule_detection_reads_body_only_when_needed() {
        let mut monitor = monitor("http://example.com".into());
        monitor.config = MonitorConfig {
            expected_headers: Some(vec![HttpHeaderMatch {
                key: "x-state".into(),
                value: "ready".into(),
            }]),
            ..MonitorConfig::default()
        };

        assert!(!should_read_body(&monitor));

        monitor.config.keyword = Some("ok".into());
        assert!(should_read_body(&monitor));
    }

    #[tokio::test]
    async fn http_probe_rejects_invalid_url_before_network_io() {
        let monitor = monitor("not a url".into());

        let error = probe(&monitor, std::time::Duration::from_secs(1))
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Other(_)));
    }

    fn monitor(target: String) -> Monitor {
        let now = Utc::now();
        Monitor {
            id: 10,
            name: "http".into(),
            kind: MonitorKind::Http,
            target,
            config: MonitorConfig::default(),
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}
