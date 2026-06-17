//! 应用配置。
//!
//! 当前版本直接从环境变量读取配置，适合个人自部署场景：
//! 默认值可开箱即用，需要调整时再通过环境变量覆盖。

use std::{env, time::Duration};

use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// HTTP 服务监听地址，默认只绑定本机。
    pub host: String,
    /// HTTP 服务监听端口。
    pub port: u16,
    /// SQLx SQLite 连接字符串，例如 `sqlite://netwatch.db`。
    pub database_url: String,
    /// 调度器扫描监控项的周期，不等同于每个监控项自身的探测间隔。
    pub scheduler_tick: Duration,
    /// 连续失败多少次后生成 down 告警。
    pub failure_threshold: u32,
    /// 聚合任务使用的日历时区，未显式配置时使用电脑当前时区。
    pub aggregation_timezone: String,
    /// 聚合任务执行周期。
    pub compact_interval: Duration,
    /// 探测结果内存缓冲批量落库周期。
    pub check_flush_interval: Duration,
    /// 可选 Webhook 地址；为空时只记录告警事件，不投递外部通知。
    pub webhook_url: Option<String>,
}

impl Config {
    /// 从环境变量加载配置，并为个人本地运行提供保守默认值。
    pub fn from_env() -> anyhow::Result<Self> {
        let host = env::var("NETWATCH_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = parse_env("NETWATCH_PORT", 4311)?;
        let database_url =
            env::var("NETWATCH_DATABASE_URL").unwrap_or_else(|_| "sqlite://netwatch.db".into());
        let scheduler_tick = Duration::from_secs(parse_env("NETWATCH_SCHEDULER_TICK_SECONDS", 5)?);
        let failure_threshold = parse_env("NETWATCH_FAILURE_THRESHOLD", 3)?;
        let aggregation_timezone = resolve_aggregation_timezone(
            env::var("NETWATCH_AGGREGATION_TIMEZONE")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            Some(system_timezone_offset()),
        );
        let compact_interval =
            Duration::from_secs(parse_env("NETWATCH_COMPACT_INTERVAL_SECONDS", 600)?);
        let check_flush_interval =
            Duration::from_secs(parse_env("NETWATCH_CHECK_FLUSH_INTERVAL_SECONDS", 60)?);
        let webhook_url = env::var("NETWATCH_WEBHOOK_URL")
            .ok()
            .filter(|value| !value.is_empty());

        Ok(Self {
            host,
            port,
            database_url,
            scheduler_tick,
            failure_threshold,
            aggregation_timezone,
            compact_interval,
            check_flush_interval,
            webhook_url,
        })
    }
}

/// 解析聚合时区：优先使用显式传入值，其次使用系统时区，最后兜底 UTC。
fn resolve_aggregation_timezone(explicit: Option<String>, system: Option<String>) -> String {
    explicit
        .or(system)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "UTC".to_string())
}

/// 将电脑当前时区转换成固定偏移字符串，供聚合日历边界使用。
fn system_timezone_offset() -> String {
    let seconds = Local::now().offset().local_minus_utc();
    format_offset(seconds).unwrap_or_else(|| "UTC".to_string())
}

/// 把秒级偏移格式化为 `+08:00` / `-05:30`。
fn format_offset(seconds: i32) -> Option<String> {
    let sign = if seconds >= 0 { '+' } else { '-' };
    let seconds = seconds.checked_abs()?;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    Some(format!("{sign}{hours:02}:{minutes:02}"))
}

fn parse_env<T>(key: &str, default: T) -> anyhow::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env::var(key) {
        Ok(value) => Ok(value.parse()?),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_self_hosted_friendly() {
        let config = Config::from_env().expect("config");
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 4311);
        assert_eq!(config.failure_threshold, 3);
        assert!(!config.aggregation_timezone.is_empty());
        assert_eq!(config.compact_interval, Duration::from_secs(600));
        assert_eq!(config.check_flush_interval, Duration::from_secs(60));
    }

    #[test]
    fn aggregation_timezone_prefers_explicit_value() {
        let timezone =
            resolve_aggregation_timezone(Some("Asia/Shanghai".to_string()), Some("+00:00".into()));

        assert_eq!(timezone, "Asia/Shanghai");
    }

    #[test]
    fn aggregation_timezone_uses_system_offset_when_explicit_is_missing() {
        let timezone = resolve_aggregation_timezone(None, Some("+08:00".to_string()));

        assert_eq!(timezone, "+08:00");
    }

    #[test]
    fn aggregation_timezone_falls_back_to_utc_when_no_source_exists() {
        let timezone = resolve_aggregation_timezone(None, None);

        assert_eq!(timezone, "UTC");
    }

    #[test]
    fn formats_local_offset_for_fixed_offset_parser() {
        assert_eq!(format_offset(8 * 60 * 60).as_deref(), Some("+08:00"));
        assert_eq!(
            format_offset(-(5 * 60 * 60 + 30 * 60)).as_deref(),
            Some("-05:30")
        );
    }
}
