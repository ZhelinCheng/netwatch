//! 探测器分发层。
//!
//! 每个协议实现自己的 `probe`，这里负责按监控项类型选择探测器，
//! 并把运行时错误统一转换为 down 结果，避免后台任务因为单次探测失败而退出。

pub mod dns;
pub mod http;
pub mod icmp;
pub mod observation;
pub mod tcp;

use std::time::Duration;

use chrono::Utc;

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
    let checked_at = Utc::now();
    let result = match monitor.kind {
        MonitorKind::Http => http::probe(monitor, timeout).await,
        MonitorKind::Ping => icmp::probe(monitor, timeout).await,
        MonitorKind::Dns => dns::probe(monitor, timeout).await,
        MonitorKind::Tcp => tcp::probe(monitor, timeout).await,
    };

    let mut result = result.unwrap_or_else(|error| {
        tracing::warn!(
            ?error,
            monitor_id = monitor.id,
            kind = monitor.kind.as_str(),
            target = %monitor.target,
            "probe failed before producing result"
        );
        CheckResult::failed_with_message(monitor.id, None, error.to_string())
    });
    // 调度间隔以探测开始时间为准，避免探测耗时把下一次 5 秒 tick 挤到 10 秒后。
    result.checked_at = checked_at;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::monitor::{MonitorConfig, MonitorKind};

    use super::*;

    #[tokio::test]
    async fn run_converts_probe_errors_to_failed_results_for_http_and_tcp() {
        let http = monitor(MonitorKind::Http, "not a url");
        let http_result = run(&http).await.unwrap();
        assert_eq!(http_result.monitor_id, http.id);
        assert_eq!(
            http_result.status,
            crate::domain::check::CheckStatus::Failed
        );

        let tcp = monitor(MonitorKind::Tcp, "missing-port");
        let tcp_result = run(&tcp).await.unwrap();
        assert_eq!(tcp_result.monitor_id, tcp.id);
        assert_eq!(tcp_result.status, crate::domain::check::CheckStatus::Failed);
    }

    fn monitor(kind: MonitorKind, target: &str) -> Monitor {
        let now = Utc::now();
        Monitor {
            id: 77,
            name: "probe".into(),
            kind,
            target: target.into(),
            config: MonitorConfig::default(),
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}
