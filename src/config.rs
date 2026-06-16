//! 应用配置。
//!
//! 当前版本直接从环境变量读取配置，适合个人自部署场景：
//! 默认值可开箱即用，需要调整时再通过环境变量覆盖。

use std::{env, time::Duration};

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
        let webhook_url = env::var("NETWATCH_WEBHOOK_URL")
            .ok()
            .filter(|value| !value.is_empty());

        Ok(Self {
            host,
            port,
            database_url,
            scheduler_tick,
            failure_threshold,
            webhook_url,
        })
    }
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
    }
}
