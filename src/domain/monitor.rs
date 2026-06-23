use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::AppError;

/// 监控项类型。
///
/// JSON 使用 snake_case，便于 REST API 直接接收 `http`、`dns` 等值。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MonitorConfig {
    /// HTTP 探测期望状态码，默认 200。
    #[serde(default)]
    pub expected_status: Option<u16>,
    /// HTTP 默认状态码下界；未设置自定义规则时默认使用 200。
    #[serde(default)]
    pub expected_status_min: Option<u16>,
    /// HTTP 默认状态码上界；未设置自定义规则时默认使用 400（不含）。
    #[serde(default)]
    pub expected_status_max: Option<u16>,
    /// HTTP 探测可选关键字；设置后响应体必须包含该字符串。
    #[serde(default)]
    pub keyword: Option<String>,
    /// HTTP 响应头匹配规则；value 按正则表达式匹配。
    #[serde(default)]
    pub expected_headers: Option<Vec<HttpHeaderMatch>>,
    /// 多条响应头规则的匹配方式，默认要求全部满足。
    #[serde(default)]
    pub header_match_mode: Option<HeaderMatchMode>,
    /// DNS 记录类型。
    #[serde(default)]
    pub dns_record: Option<DnsRecordType>,
    /// DNS 期望解析结果；设置后至少一个解析值需要精确匹配。
    #[serde(default)]
    pub expected_value: Option<String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            expected_status: None,
            expected_status_min: None,
            expected_status_max: None,
            keyword: None,
            expected_headers: None,
            header_match_mode: Some(HeaderMatchMode::All),
            dns_record: Some(DnsRecordType::A),
            expected_value: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct HttpHeaderMatch {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeaderMatchMode {
    /// 全部响应头规则都必须满足。
    All,
    /// 任一响应头规则满足即可。
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum DnsRecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    NS,
    SOA,
    CAA,
    SRV,
}

impl DnsRecordType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::A => "A",
            Self::AAAA => "AAAA",
            Self::CNAME => "CNAME",
            Self::MX => "MX",
            Self::TXT => "TXT",
            Self::NS => "NS",
            Self::SOA => "SOA",
            Self::CAA => "CAA",
            Self::SRV => "SRV",
        }
    }
}

/// 监控项的完整持久化模型。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Monitor {
    /// SQLite 自增主键；新监控项写入前为 0。
    pub id: i64,
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
    #[serde(with = "chrono::serde::ts_seconds")]
    #[schema(value_type = i64)]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    #[schema(value_type = i64)]
    pub updated_at: DateTime<Utc>,
}

/// 创建监控项的 API 输入。
#[derive(Debug, Clone, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UpdateMonitor {
    pub name: Option<String>,
    pub target: Option<String>,
    pub config: Option<MonitorConfig>,
    pub interval_seconds: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub enabled: Option<bool>,
}

impl CreateMonitor {
    /// 将用户输入转换成完整领域模型，并填充时间戳。
    pub fn into_monitor(self) -> Result<Monitor, AppError> {
        validate_monitor_input(
            &self.name,
            &self.target,
            self.kind.clone(),
            &self.config,
            self.interval_seconds,
            self.timeout_seconds,
        )?;

        let now = Utc::now();
        Ok(Monitor {
            id: 0,
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
    kind: MonitorKind,
    config: &MonitorConfig,
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
    if interval_seconds < 2 {
        return Err(AppError::BadRequest(
            "interval_seconds must be at least 2".to_string(),
        ));
    }
    if timeout_seconds == 0 || timeout_seconds > interval_seconds {
        return Err(AppError::BadRequest(
            "timeout_seconds must be greater than 0 and not exceed interval_seconds".to_string(),
        ));
    }
    validate_monitor_config(kind, config)?;
    Ok(())
}

fn validate_monitor_config(kind: MonitorKind, config: &MonitorConfig) -> Result<(), AppError> {
    if kind != MonitorKind::Http {
        return Ok(());
    }

    if let Some(keyword) = config.keyword.as_deref().filter(|value| !value.is_empty()) {
        regex::Regex::new(keyword)
            .map_err(|error| AppError::BadRequest(format!("keyword regex is invalid: {error}")))?;
    }

    for header in config.expected_headers.as_deref().unwrap_or_default() {
        if header.key.trim().is_empty() {
            return Err(AppError::BadRequest(
                "header match key must not be empty".to_string(),
            ));
        }
        if header.value.trim().is_empty() {
            return Err(AppError::BadRequest(
                "header match value must not be empty".to_string(),
            ));
        }
        regex::Regex::new(header.value.trim()).map_err(|error| {
            AppError::BadRequest(format!("header match regex is invalid: {error}"))
        })?;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn monitor_kind_round_trips_database_strings() {
        assert_eq!(MonitorKind::Http.as_str(), "http");
        assert_eq!(MonitorKind::Ping.as_str(), "ping");
        assert_eq!(MonitorKind::Dns.as_str(), "dns");
        assert_eq!(MonitorKind::Tcp.as_str(), "tcp");
        assert_eq!(MonitorKind::try_from("http").unwrap(), MonitorKind::Http);
        assert_eq!(MonitorKind::try_from("ping").unwrap(), MonitorKind::Ping);
        assert_eq!(MonitorKind::try_from("dns").unwrap(), MonitorKind::Dns);
        assert_eq!(MonitorKind::try_from("tcp").unwrap(), MonitorKind::Tcp);
        assert!(MonitorKind::try_from("smtp").is_err());
    }

    #[test]
    fn monitor_config_defaults_are_stable() {
        let config = MonitorConfig::default();
        assert_eq!(config.dns_record, Some(DnsRecordType::A));
        assert_eq!(config.header_match_mode, Some(HeaderMatchMode::All));
        assert_eq!(DnsRecordType::TXT.as_str(), "TXT");
    }

    #[test]
    fn create_monitor_fills_runtime_fields_and_validates_input() {
        let monitor = CreateMonitor {
            name: "site".into(),
            kind: MonitorKind::Http,
            target: "https://example.com".into(),
            config: MonitorConfig::default(),
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
        }
        .into_monitor()
        .unwrap();

        assert_eq!(monitor.id, 0);
        assert_eq!(monitor.kind, MonitorKind::Http);
        assert!(monitor.enabled);

        assert!(
            validate_monitor_input("", "x", MonitorKind::Http, &MonitorConfig::default(), 5, 1)
                .is_err()
        );
        assert!(
            validate_monitor_input("x", " ", MonitorKind::Http, &MonitorConfig::default(), 5, 1)
                .is_err()
        );
        assert!(
            validate_monitor_input("x", "y", MonitorKind::Http, &MonitorConfig::default(), 1, 1)
                .is_err()
        );
        assert!(
            validate_monitor_input("x", "y", MonitorKind::Http, &MonitorConfig::default(), 2, 1)
                .is_ok()
        );
        assert!(
            validate_monitor_input("x", "y", MonitorKind::Http, &MonitorConfig::default(), 5, 0)
                .is_err()
        );
        assert!(
            validate_monitor_input("x", "y", MonitorKind::Http, &MonitorConfig::default(), 5, 6)
                .is_err()
        );
    }

    #[test]
    fn monitor_config_validates_http_regex_and_headers() {
        let config = MonitorConfig {
            keyword: Some("[".into()),
            ..MonitorConfig::default()
        };
        assert!(validate_monitor_config(MonitorKind::Http, &config).is_err());

        let config = MonitorConfig {
            expected_headers: Some(vec![HttpHeaderMatch {
                key: "x-state".into(),
                value: "rea.*".into(),
            }]),
            ..MonitorConfig::default()
        };
        assert!(validate_monitor_config(MonitorKind::Http, &config).is_ok());

        let config = MonitorConfig {
            expected_headers: Some(vec![HttpHeaderMatch {
                key: "".into(),
                value: "ready".into(),
            }]),
            ..MonitorConfig::default()
        };
        assert!(validate_monitor_config(MonitorKind::Http, &config).is_err());
    }

    #[test]
    fn create_monitor_deserializes_defaults_from_api_payload() {
        let input: CreateMonitor = serde_json::from_value(json!({
            "name": "site",
            "kind": "http",
            "target": "https://example.com"
        }))
        .unwrap();

        assert_eq!(input.interval_seconds, 60);
        assert_eq!(input.timeout_seconds, 10);
        assert!(input.enabled);
        assert_eq!(input.config.dns_record, Some(DnsRecordType::A));
    }

    #[test]
    fn monitor_config_ignores_removed_success_rules_from_old_json() {
        let config: MonitorConfig = serde_json::from_value(json!({
            "success_rules": [{ "type": "latency", "op": "lt", "value_us": 500 }],
            "dns_record": "TXT"
        }))
        .unwrap();

        assert_eq!(config.dns_record, Some(DnsRecordType::TXT));
    }
}
