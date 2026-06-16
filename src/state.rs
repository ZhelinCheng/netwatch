//! 进程级共享状态。
//!
//! Axum handler 和后台 scheduler 都通过 `AppState` 访问配置和数据库连接池。

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub struct AppStateInner {
    /// 启动时解析得到的只读配置。
    pub config: Config,
    /// SQLite 连接池，可安全克隆并跨 async task 使用。
    pub pool: SqlitePool,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            inner: Arc::new(AppStateInner { config, pool }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }
}
