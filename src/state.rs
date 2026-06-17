//! 进程级共享状态。
//!
//! Axum handler 和后台 scheduler 都通过 `AppState` 访问配置和数据库连接池。

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::{config::Config, domain::check::CheckResult};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    /// 启动时解析得到的只读配置。
    pub config: Config,
    /// SQLite 连接池，可安全克隆并跨 async task 使用。
    pub pool: SqlitePool,
    /// 探测结果写库前的短期内存缓冲。
    pub check_buffer: CheckResultBuffer,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config,
                pool,
                check_buffer: CheckResultBuffer::default(),
            }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    pub fn check_buffer(&self) -> &CheckResultBuffer {
        &self.inner.check_buffer
    }
}

#[derive(Default)]
pub struct CheckResultBuffer {
    pending: Mutex<Vec<CheckResult>>,
}

impl CheckResultBuffer {
    pub async fn append(&self, result: CheckResult) {
        self.pending.lock().await.push(result);
    }

    pub async fn latest_for_monitor(&self, monitor_id: &str) -> Option<CheckResult> {
        self.pending
            .lock()
            .await
            .iter()
            .filter(|result| result.monitor_id == monitor_id)
            .max_by_key(|result| result.checked_at)
            .cloned()
    }

    pub async fn latest_by_monitor(&self) -> HashMap<String, CheckResult> {
        let mut latest = HashMap::new();
        for result in self.pending.lock().await.iter() {
            let entry = latest
                .entry(result.monitor_id.clone())
                .or_insert_with(|| result.clone());
            if result.checked_at > entry.checked_at {
                *entry = result.clone();
            }
        }
        latest
    }

    pub async fn list_for_monitor_between(
        &self,
        monitor_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<CheckResult> {
        self.pending
            .lock()
            .await
            .iter()
            .filter(|result| {
                result.monitor_id == monitor_id
                    && result.checked_at >= from
                    && result.checked_at <= to
            })
            .cloned()
            .collect()
    }

    pub async fn drain_all(&self) -> Vec<CheckResult> {
        std::mem::take(&mut *self.pending.lock().await)
    }

    pub async fn requeue_front(&self, mut results: Vec<CheckResult>) {
        let mut pending = self.pending.lock().await;
        results.append(&mut pending);
        *pending = results;
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::domain::check::CheckResult;

    use super::*;

    #[tokio::test]
    async fn buffer_tracks_latest_and_preserves_failed_flush_results() {
        let buffer = CheckResultBuffer::default();
        let mut first = CheckResult::success("m1".into(), 10);
        first.checked_at = Utc::now();
        let mut second = CheckResult::failed("m1".into(), None);
        second.checked_at = first.checked_at + Duration::seconds(5);

        buffer.append(first.clone()).await;
        buffer.append(second.clone()).await;

        assert_eq!(
            buffer.latest_for_monitor("m1").await.unwrap().checked_at,
            second.checked_at
        );

        let drained = buffer.drain_all().await;
        assert_eq!(drained.len(), 2);
        assert!(buffer.latest_for_monitor("m1").await.is_none());

        buffer.requeue_front(drained).await;
        assert_eq!(buffer.drain_all().await.len(), 2);
    }
}
