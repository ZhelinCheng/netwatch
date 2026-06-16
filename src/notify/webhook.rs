//! Webhook 通知实现。

use reqwest::Client;
use serde_json::json;

use crate::domain::{alert::AlertEvent, monitor::Monitor};

/// 以 JSON 格式投递监控项和告警事件。
pub async fn send(url: &str, monitor: &Monitor, event: &AlertEvent) -> anyhow::Result<()> {
    Client::new()
        .post(url)
        .json(&json!({
            "monitor": monitor,
            "event": event
        }))
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
