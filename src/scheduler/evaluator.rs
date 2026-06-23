//! 告警评估。
//!
//! 第一版规则很克制：连续 failed 达到阈值触发 down，down 后首次 success 触发 recovered。

use chrono::Utc;

use crate::{
    domain::{
        alert::{AlertEvent, AlertKind},
        check::{CheckResult, CheckStatus},
        monitor::Monitor,
    },
    notify,
    state::AppState,
    storage::{alerts, checks},
};

pub async fn evaluate(
    state: &AppState,
    monitor: &Monitor,
    result: &CheckResult,
) -> anyhow::Result<Option<AlertEvent>> {
    // 只读取阈值数量的最近结果即可判断是否连续失败。
    let recent = checks::list_for_monitor(
        state.pool(),
        monitor.id,
        state.config().failure_threshold as i64,
    )
    .await?;
    let latest_alert = alerts::latest_for_monitor(state.pool(), monitor.id).await?;

    let event = if result.status == CheckStatus::Failed
        && recent.len() == state.config().failure_threshold as usize
        && recent.iter().all(|item| item.status == CheckStatus::Failed)
        && !matches!(
            latest_alert.as_ref().map(|event| &event.kind),
            Some(AlertKind::Triggered)
        ) {
        Some(AlertEvent {
            id: None,
            monitor_id: monitor.id,
            kind: AlertKind::Triggered,
            message: format!("{} is failing", monitor.name),
            delivered: false,
            created_at: Utc::now(),
        })
    } else if result.status == CheckStatus::Success
        && matches!(
            latest_alert.as_ref().map(|event| &event.kind),
            Some(AlertKind::Triggered)
        )
    {
        Some(AlertEvent {
            id: None,
            monitor_id: monitor.id,
            kind: AlertKind::Recovered,
            message: format!("{} has recovered", monitor.name),
            delivered: false,
            created_at: Utc::now(),
        })
    } else {
        None
    };

    if let Some(mut event) = event {
        tracing::info!(
            monitor_id = monitor.id,
            name = %monitor.name,
            alert_kind = event.kind.as_str(),
            "alert event generated"
        );
        event.delivered = match notify::send(state, monitor, &event).await {
            Ok(delivered) => delivered,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    monitor_id = monitor.id,
                    alert_kind = event.kind.as_str(),
                    "alert notification failed"
                );
                false
            }
        };
        alerts::insert(state.pool(), &event).await?;
        tracing::info!(
            monitor_id = monitor.id,
            alert_kind = event.kind.as_str(),
            delivered = event.delivered,
            "alert event persisted"
        );
        return Ok(Some(event));
    }

    tracing::debug!(
        monitor_id = monitor.id,
        status = result.status.as_str(),
        recent_count = recent.len(),
        "alert evaluation produced no event"
    );
    Ok(None)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use crate::{domain::monitor::MonitorKind, storage::monitors, test_support};

    use super::*;

    #[test]
    fn triggered_alert_is_not_repeated() {
        let latest = Some(AlertEvent {
            id: Some(1),
            monitor_id: 1,
            kind: AlertKind::Triggered,
            message: "down".into(),
            delivered: true,
            created_at: Utc::now(),
        });

        assert!(matches!(
            latest.as_ref().map(|event| &event.kind),
            Some(AlertKind::Triggered)
        ));
    }

    #[tokio::test]
    async fn evaluate_triggers_once_after_threshold_and_recovers() {
        let state = test_support::state("evaluator-flow").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let now = Utc::now();

        let mut first = CheckResult::failed(monitor.id, None);
        first.checked_at = now;
        let mut second = CheckResult::failed(monitor.id, None);
        second.checked_at = now + Duration::seconds(5);
        let mut third = CheckResult::failed(monitor.id, None);
        third.checked_at = now + Duration::seconds(10);
        persist(&state, &[first.clone(), second.clone(), third.clone()]).await;

        let event = evaluate(&state, &monitor, &third).await.unwrap().unwrap();
        assert_eq!(event.kind, AlertKind::Triggered);
        assert!(!event.delivered);

        let repeated = evaluate(&state, &monitor, &third).await.unwrap();
        assert!(repeated.is_none());

        let mut success = CheckResult::success(monitor.id, 10);
        success.checked_at = now + Duration::seconds(15);
        persist(&state, &[success.clone()]).await;
        let recovered = evaluate(&state, &monitor, &success).await.unwrap().unwrap();
        assert_eq!(recovered.kind, AlertKind::Recovered);
    }

    #[tokio::test]
    async fn evaluate_ignores_incomplete_failure_streak() {
        let state = test_support::state("evaluator-incomplete").await;
        let monitor = monitors::insert(state.pool(), &test_support::monitor(MonitorKind::Http))
            .await
            .unwrap();
        let result = CheckResult::failed(monitor.id, None);
        persist(&state, std::slice::from_ref(&result)).await;

        assert!(evaluate(&state, &monitor, &result).await.unwrap().is_none());
    }

    async fn persist(state: &AppState, results: &[CheckResult]) {
        let mut tx = state.pool().begin().await.unwrap();
        checks::insert_many_tx(&mut tx, results).await.unwrap();
        tx.commit().await.unwrap();
    }
}
