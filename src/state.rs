//! 进程级共享状态。
//!
//! Axum handler 和后台 scheduler 都通过 `AppState` 访问配置和数据库连接池。

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    config::Config,
    domain::{check::CheckResult, monitor::Monitor},
};

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
    /// 调度器使用的监控项快照，避免每秒 tick 都访问数据库。
    pub monitor_cache: MonitorCache,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config,
                pool,
                check_buffer: CheckResultBuffer::default(),
                monitor_cache: MonitorCache::default(),
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

    pub fn monitor_cache(&self) -> &MonitorCache {
        &self.inner.monitor_cache
    }
}

#[derive(Default)]
pub struct CheckResultBuffer {
    pending: Mutex<Vec<CheckResult>>,
}

pub struct MonitorCache {
    inner: Mutex<MonitorCacheInner>,
}

struct MonitorCacheInner {
    monitors: Vec<Monitor>,
    dirty: bool,
}

impl Default for MonitorCache {
    fn default() -> Self {
        Self {
            inner: Mutex::new(MonitorCacheInner {
                monitors: Vec::new(),
                dirty: true,
            }),
        }
    }
}

impl MonitorCache {
    pub async fn snapshot(&self) -> Option<Vec<Monitor>> {
        let inner = self.inner.lock().await;
        if inner.dirty {
            return None;
        }
        Some(inner.monitors.clone())
    }

    pub async fn replace(&self, monitors: Vec<Monitor>) {
        let mut inner = self.inner.lock().await;
        inner.monitors = monitors;
        inner.dirty = false;
    }

    pub async fn mark_dirty(&self) {
        self.inner.lock().await.dirty = true;
    }
}

impl CheckResultBuffer {
    pub async fn append(&self, result: CheckResult) {
        self.pending.lock().await.push(result);
    }

    pub async fn latest_for_monitor(&self, monitor_id: i64) -> Option<CheckResult> {
        self.pending
            .lock()
            .await
            .iter()
            .filter(|result| result.monitor_id == monitor_id)
            .max_by_key(|result| result.checked_at)
            .cloned()
    }

    pub async fn latest_by_monitor(&self) -> HashMap<i64, CheckResult> {
        let mut latest = HashMap::new();
        for result in self.pending.lock().await.iter() {
            let entry = latest
                .entry(result.monitor_id)
                .or_insert_with(|| result.clone());
            if result.checked_at > entry.checked_at {
                *entry = result.clone();
            }
        }
        latest
    }

    pub async fn list_for_monitor_between(
        &self,
        monitor_id: i64,
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

    use crate::domain::{
        check::CheckResult,
        monitor::{MonitorConfig, MonitorKind},
    };

    use super::*;

    #[tokio::test]
    async fn buffer_tracks_latest_and_preserves_failed_flush_results() {
        let buffer = CheckResultBuffer::default();
        let mut first = CheckResult::success(1, 10);
        first.checked_at = Utc::now();
        let mut second = CheckResult::failed(1, None);
        second.checked_at = first.checked_at + Duration::seconds(5);

        buffer.append(first.clone()).await;
        buffer.append(second.clone()).await;

        assert_eq!(
            buffer.latest_for_monitor(1).await.unwrap().checked_at,
            second.checked_at
        );

        let drained = buffer.drain_all().await;
        assert_eq!(drained.len(), 2);
        assert!(buffer.latest_for_monitor(1).await.is_none());

        buffer.requeue_front(drained).await;
        assert_eq!(buffer.drain_all().await.len(), 2);
    }

    #[tokio::test]
    async fn monitor_cache_returns_snapshot_until_dirty() {
        let cache = MonitorCache::default();
        assert!(cache.snapshot().await.is_none());

        cache.replace(vec![monitor(1)]).await;
        assert_eq!(cache.snapshot().await.unwrap().len(), 1);

        cache.mark_dirty().await;
        assert!(cache.snapshot().await.is_none());
    }

    fn monitor(id: i64) -> Monitor {
        let now = Utc::now();
        Monitor {
            id,
            name: format!("m{id}"),
            kind: MonitorKind::Http,
            target: "https://example.com".into(),
            config: MonitorConfig::default(),
            interval_seconds: 60,
            timeout_seconds: 10,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}
