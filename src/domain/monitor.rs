use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// 监控项类型。
///
/// JSON 使用 snake_case，便于 REST API 直接接收 `http`、`dns` 等值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorKind {
    Http,
    Ping,
    Dns,
    Tcp,
}

impl MonitorKind {
    /// 持久化到数据库时使用的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Ping => "ping",
            Self::Dns => "dns",
            Self::Tcp => "tcp",
        }
    }
}

impl TryFrom<&str> for MonitorKind {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "http" => Ok(Self::Http),
            "ping" => Ok(Self::Ping),
            "dns" => Ok(Self::Dns),
            "tcp" => Ok(Self::Tcp),
            other => Err(AppError::BadRequest(format!(
                "unknown monitor kind: {other}"
            ))),
        }
    }
}

/// 各协议共享的一组轻量配置。
///
/// 第一版把协议特定字段放在同一个结构里，避免过早引入复杂的枚举配置迁移。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// HTTP 探测期望状态码，默认 200。
    #[serde(default)]
    pub expected_status: Option<u16>,
    /// HTTP 探测可选关键字；设置后响应体必须包含该字符串。
    #[serde(default)]
    pub keyword: Option<String>,
    /// DNS 记录类型提示；当前实现主要用于元数据展示。
    #[serde(default)]
    pub dns_record: Option<String>,
    /// DNS 期望解析结果；设置后至少一个解析值需要精确匹配。
    #[serde(default)]
    pub expected_value: Option<String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            expected_status: Some(200),
            keyword: None,
            dns_record: Some("A".to_string()),
            expected_value: None,
        }
    }
}

/// 监控项的完整持久化模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    /// UUID 字符串，便于 API 和 SQLite 直接使用。
    pub id: String,
    pub name: String,
    pub kind: MonitorKind,
    /// 被探测目标；格式由具体探测器解释。
    pub target: String,
    pub config: MonitorConfig,
    /// 单个监控项的最小探测间隔。
    pub interval_seconds: u64,
    /// 单次探测超时时间，必须小于探测间隔。
    pub timeout_seconds: u64,
    /// 关闭后调度器会跳过该监控项。
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 创建监控项的 API 输入。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMonitor {
    pub name: String,
    pub kind: MonitorKind,
    pub target: String,
    #[serde(default)]
    pub config: MonitorConfig,
    #[serde(default = "default_interval_seconds")]
    pub interval_seconds: u64,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// 更新监控项的 API 输入；未传字段保持原值。
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateMonitor {
    pub name: Option<String>,
    pub target: Option<String>,
    pub config: Option<MonitorConfig>,
    pub interval_seconds: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub enabled: Option<bool>,
}

impl CreateMonitor {
    /// 将用户输入转换成完整领域模型，并填充 ID 与时间戳。
    pub fn into_monitor(self) -> Result<Monitor, AppError> {
        validate_monitor_input(
            &self.name,
            &self.target,
            self.interval_seconds,
            self.timeout_seconds,
        )?;

        let now = Utc::now();
        Ok(Monitor {
            id: Uuid::new_v4().to_string(),
            name: self.name,
            kind: self.kind,
            target: self.target,
            config: self.config,
            interval_seconds: self.interval_seconds,
            timeout_seconds: self.timeout_seconds,
            enabled: self.enabled,
            created_at: now,
            updated_at: now,
        })
    }
}

/// 校验会影响调度稳定性的公共输入。
pub fn validate_monitor_input(
    name: &str,
    target: &str,
    interval_seconds: u64,
    timeout_seconds: u64,
) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::BadRequest("monitor name is required".to_string()));
    }
    if target.trim().is_empty() {
        return Err(AppError::BadRequest(
            "monitor target is required".to_string(),
        ));
    }
    if interval_seconds < 5 {
        return Err(AppError::BadRequest(
            "interval_seconds must be at least 5".to_string(),
        ));
    }
    if timeout_seconds == 0 || timeout_seconds >= interval_seconds {
        return Err(AppError::BadRequest(
            "timeout_seconds must be greater than 0 and lower than interval_seconds".to_string(),
        ));
    }
    Ok(())
}

fn default_interval_seconds() -> u64 {
    60
}

fn default_timeout_seconds() -> u64 {
    10
}

fn default_enabled() -> bool {
    true
}
