//! 通知分发层。
//!
//! 当前只支持 Webhook；没有配置通知地址时，告警仍会写入数据库。

pub mod webhook;

use crate::{
    domain::{alert::AlertEvent, monitor::Monitor},
    state::AppState,
};

/// 发送告警通知，返回是否实际投递到外部渠道。
pub async fn send(state: &AppState, monitor: &Monitor, event: &AlertEvent) -> anyhow::Result<bool> {
    if let Some(url) = &state.config().webhook_url {
        tracing::info!(
            monitor_id = monitor.id,
            alert_kind = event.kind.as_str(),
            "sending webhook notification"
        );
        webhook::send(url, monitor, event).await?;
        return Ok(true);
    }
    tracing::info!(
        monitor_id = monitor.id,
        alert_kind = event.kind.as_str(),
        "webhook notification skipped because url is not configured"
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::{
        domain::{
            alert::{AlertEvent, AlertKind},
            monitor::MonitorKind,
        },
        test_support,
    };

    use super::*;

    #[tokio::test]
    async fn send_skips_external_delivery_when_webhook_is_absent() {
        let state = test_support::state("notify-skip").await;
        let monitor = test_support::monitor(MonitorKind::Http);
        let event = AlertEvent {
            id: None,
            monitor_id: monitor.id,
            kind: AlertKind::Triggered,
            message: "down".into(),
            delivered: false,
            created_at: Utc::now(),
        };

        let delivered = send(&state, &monitor, &event).await.unwrap();

        assert!(!delivered);
    }
}
