//! TCP 连通性探测器。

use std::time::Instant;

use tokio::{
    net::{TcpStream, lookup_host},
    time,
};
use url::Url;

use crate::{
    domain::{check::CheckResult, monitor::Monitor},
    error::AppError,
};

pub async fn probe(
    monitor: &Monitor,
    timeout: std::time::Duration,
) -> Result<CheckResult, AppError> {
    let address = parse_target(&monitor.target)?;
    let started = Instant::now();
    tracing::debug!(
        monitor_id = monitor.id,
        address = %address,
        timeout_ms = timeout.as_millis(),
        "starting tcp probe"
    );
    let mut addresses = time::timeout(timeout, lookup_host(&address))
        .await
        .map_err(|_| AppError::BadRequest("tcp target resolution timed out".to_string()))?
        .map_err(anyhow::Error::from)?;
    let socket = addresses
        .next()
        .ok_or_else(|| AppError::BadRequest("tcp target resolved to no addresses".to_string()))?;

    time::timeout(timeout, TcpStream::connect(socket))
        .await
        .map_err(|_| AppError::BadRequest("tcp connection timed out".to_string()))?
        .map_err(anyhow::Error::from)?;

    Ok(CheckResult::success(
        monitor.id,
        started.elapsed().as_micros() as u64,
    ))
}

/// 支持 `host:port`、`ip:port` 和带 scheme 的 URL。
fn parse_target(target: &str) -> Result<String, AppError> {
    if target.rsplit_once(':').is_some() && !target.contains("://") {
        return Ok(target.to_string());
    }

    let url = Url::parse(target).map_err(|_| {
        AppError::BadRequest(
            "tcp target must be host:port or URL with an explicit port".to_string(),
        )
    })?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("tcp target URL is missing host".to_string()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppError::BadRequest("tcp target URL is missing port".to_string()))?;
    Ok(format!("{host}:{port}"))
}
