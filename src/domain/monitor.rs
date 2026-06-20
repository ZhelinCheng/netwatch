use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// HTTP 默认状态码下界；未设置自定义规则时默认使用 200。
    #[serde(default)]
    pub expected_status_min: Option<u16>,
    /// HTTP 默认状态码上界；未设置自定义规则时默认使用 400（不含）。
    #[serde(default)]
    pub expected_status_max: Option<u16>,
    /// HTTP 探测可选关键字；设置后响应体必须包含该字符串。
    #[serde(default)]
    pub keyword: Option<String>,
    /// DNS 记录类型提示；当前实现主要用于元数据展示。
    #[serde(default)]
    pub dns_record: Option<String>,
    /// DNS 期望解析结果；设置后至少一个解析值需要精确匹配。
    #[serde(default)]
    pub expected_value: Option<String>,
    /// 用户自定义成功判断规则；为空时使用协议默认判断。
    #[serde(default)]
    pub success_rules: Option<Vec<SuccessRule>>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            expected_status: None,
            expected_status_min: None,
            expected_status_max: None,
            keyword: None,
            dns_record: Some("A".to_string()),
            expected_value: None,
            success_rules: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    /// 等于。
    Eq,
    /// 不等于。
    Ne,
    /// 大于。
    Gt,
    /// 大于等于。
    Gte,
    /// 小于。
    Lt,
    /// 小于等于。
    Lte,
}

impl CompareOp {
    /// 对数值型观测值执行比较。
    pub fn matches_u64(&self, left: u64, right: u64) -> bool {
        match self {
            Self::Eq => left == right,
            Self::Ne => left != right,
            Self::Gt => left > right,
            Self::Gte => left >= right,
            Self::Lt => left < right,
            Self::Lte => left <= right,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextOp {
    /// 包含目标文本。
    Contains,
    /// 与目标文本完全一致。
    Equals,
    /// 不包含目标文本。
    NotContains,
    /// 与目标文本不一致。
    NotEquals,
}

impl TextOp {
    /// 对单个文本值执行匹配。
    pub fn matches(&self, left: &str, right: &str) -> bool {
        match self {
            Self::Contains => left.contains(right),
            Self::Equals => left == right,
            Self::NotContains => !left.contains(right),
            Self::NotEquals => left != right,
        }
    }

    /// 对一组候选文本执行匹配；否定操作要求所有候选都不命中。
    pub fn matches_any<'a>(
        &self,
        values: impl IntoIterator<Item = &'a str>,
        expected: &str,
    ) -> bool {
        match self {
            Self::NotContains | Self::NotEquals => values
                .into_iter()
                .all(|value| self.matches(value, expected)),
            Self::Contains | Self::Equals => values
                .into_iter()
                .any(|value| self.matches(value, expected)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SuccessRule {
    /// HTTP 状态码规则。
    HttpStatus { op: CompareOp, value: u16 },
    /// HTTP 响应体文本规则。
    HttpBody { op: TextOp, value: String },
    /// HTTP 响应头规则，key 会按小写匹配。
    HttpHeader {
        key: String,
        op: TextOp,
        value: String,
    },
    /// DNS 解析结果规则。
    DnsAnswer { op: TextOp, value: String },
    /// 通用延迟规则，可叠加在任意协议上。
    Latency { op: CompareOp, value_us: u64 },
}

impl MonitorConfig {
    /// 是否启用了用户自定义成功规则。
    pub fn has_success_rules(&self) -> bool {
        self.success_rules
            .as_ref()
            .is_some_and(|rules| !rules.is_empty())
    }
}

/// 监控项的完整持久化模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
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
    /// 将用户输入转换成完整领域模型，并填充时间戳。
    pub fn into_monitor(self) -> Result<Monitor, AppError> {
        validate_monitor_input(
            &self.name,
            &self.target,
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
