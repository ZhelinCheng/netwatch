//! 探测器分发层。
//!
//! 每个协议实现自己的 `probe`，这里负责按监控项类型选择探测器，
//! 并把运行时错误统一转换为 down 结果，避免后台任务因为单次探测失败而退出。

pub mod dns;
pub mod http;
pub mod icmp;
pub mod tcp;

use std::time::Duration;

use crate::{
    domain::{
        check::CheckResult,
        monitor::{Monitor, MonitorKind},
    },
    error::AppError,
};

/// 执行一次探测并返回标准化结果。
pub async fn run(monitor: &Monitor) -> Result<CheckResult, AppError> {
    let timeout = Duration::from_secs(monitor.timeout_seconds);
    let result = match monitor.kind {
        MonitorKind::Http => http::probe(monitor, timeout).await,
        MonitorKind::Ping => icmp::probe(monitor, timeout).await,
        MonitorKind::Dns => dns::probe(monitor, timeout).await,
        MonitorKind::Tcp => tcp::probe(monitor, timeout).await,
    };

    Ok(result.unwrap_or_else(|err| {
        CheckResult::down(
            monitor.id.clone(),
            err.to_string(),
            serde_json::json!({ "target": monitor.target }),
        )
    }))
}
