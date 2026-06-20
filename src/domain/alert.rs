use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 告警事件类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Triggered,
    Recovered,
    CertificateExpiring,
}

impl AlertKind {
    /// 持久化到数据库时使用的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Triggered => "triggered",
            Self::Recovered => "recovered",
            Self::CertificateExpiring => "certificate_expiring",
        }
    }
}

impl From<&str> for AlertKind {
    fn from(value: &str) -> Self {
        match value {
            "recovered" => Self::Recovered,
            "certificate_expiring" => Self::CertificateExpiring,
            _ => Self::Triggered,
        }
    }
}

/// 告警事件。
///
/// 事件采用 append-only 方式记录，方便之后扩展告警历史和通知审计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub id: Option<i64>,
    pub monitor_id: i64,
    pub kind: AlertKind,
    pub message: String,
    pub delivered: bool,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}
