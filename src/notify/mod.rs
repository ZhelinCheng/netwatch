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
        webhook::send(url, monitor, event).await?;
        return Ok(true);
    }
    Ok(false)
}
