use std::collections::HashMap;

use crate::domain::monitor::{HeaderMatchMode, Monitor, MonitorKind};

/// 各协议探测器提取出的统一观测值。
///
/// 成功规则只依赖这个结构，避免 HTTP/DNS/TCP 等探测器各自重复实现规则判断。
#[derive(Debug, Default)]
pub struct ProbeObservation {
    /// HTTP 响应状态码。
    pub http_status: Option<u16>,
    /// 需要关键字或 body 规则时才读取响应体。
    pub http_body: Option<String>,
    /// 小写化后的 HTTP 响应头。
    pub http_headers: HashMap<String, String>,
    /// DNS 解析得到的 IP 或记录值。
    pub dns_answers: Vec<String>,
}

impl ProbeObservation {
    /// 初始化观测对象，其它字段由具体探测器补充。
    pub fn new() -> Self {
        Self::default()
    }
}

/// 判断一次探测是否满足成功条件。
pub fn is_success(monitor: &Monitor, observation: &ProbeObservation) -> bool {
    default_success(monitor, observation)
}

/// 未配置自定义规则时，各协议使用的默认成功判断。
fn default_success(monitor: &Monitor, observation: &ProbeObservation) -> bool {
    match monitor.kind {
        MonitorKind::Http => {
            let Some(status) = observation.http_status else {
                return false;
            };

            if let Some(expected) = monitor.config.expected_status {
                if status != expected {
                    return false;
                }
            } else {
                let min = monitor.config.expected_status_min.unwrap_or(200);
                let max = monitor.config.expected_status_max.unwrap_or(400);
                if status < min || status >= max {
                    return false;
                }
            }

            if let Some(keyword) = &monitor.config.keyword {
                let Some(body) = observation.http_body.as_deref() else {
                    return false;
                };
                let Ok(regex) = regex::Regex::new(keyword) else {
                    return false;
                };
                if !regex.is_match(body) {
                    return false;
                }
            }

            headers_match(monitor, observation)
        }
        MonitorKind::Ping | MonitorKind::Tcp => true,
        MonitorKind::Dns => {
            if observation.dns_answers.is_empty() {
                return false;
            }
            if let Some(expected) = &monitor.config.expected_value {
                observation
                    .dns_answers
                    .iter()
                    .any(|value| value == expected)
            } else {
                true
            }
        }
    }
}

fn headers_match(monitor: &Monitor, observation: &ProbeObservation) -> bool {
    let rules = monitor
        .config
        .expected_headers
        .as_deref()
        .unwrap_or_default();
    if rules.is_empty() {
        return true;
    }

    let rule_matches = |key: &str, value: &str| {
        let normalized_key = key.to_ascii_lowercase();
        let Some(header_value) = observation.http_headers.get(&normalized_key) else {
            return false;
        };
        regex::Regex::new(value)
            .map(|regex| regex.is_match(header_value))
            .unwrap_or(false)
    };

    match monitor.config.header_match_mode {
        Some(HeaderMatchMode::Any) => rules
            .iter()
            .any(|rule| rule_matches(rule.key.trim(), rule.value.trim())),
        _ => rules
            .iter()
            .all(|rule| rule_matches(rule.key.trim(), rule.value.trim())),
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::monitor::{
        HeaderMatchMode, HttpHeaderMatch, Monitor, MonitorConfig, MonitorKind,
    };

    use super::*;

    fn monitor(kind: MonitorKind, config: MonitorConfig) -> Monitor {
        let now = chrono::Utc::now();
        Monitor {
            id: 1,
            name: "m1".into(),
            kind,
            target: "example.com".into(),
            config,
            interval_seconds: 60,
            timeout_seconds: 10,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn http_default_accepts_2xx_and_3xx() {
        let monitor = monitor(MonitorKind::Http, MonitorConfig::default());
        let mut observation = ProbeObservation::new();
        observation.http_status = Some(302);
        assert!(is_success(&monitor, &observation));

        observation.http_status = Some(404);
        assert!(!is_success(&monitor, &observation));
    }

    #[test]
    fn http_config_checks_body_and_headers() {
        let monitor = monitor(
            MonitorKind::Http,
            MonitorConfig {
                keyword: Some("all\\s+ok".into()),
                expected_headers: Some(vec![HttpHeaderMatch {
                    key: "x-state".into(),
                    value: "rea.*".into(),
                }]),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new();
        observation.http_status = Some(200);
        observation.http_body = Some("all ok".into());
        observation
            .http_headers
            .insert("x-state".into(), "ready".into());
        assert!(is_success(&monitor, &observation));
    }

    #[test]
    fn http_header_match_mode_any_accepts_one_match() {
        let monitor = monitor(
            MonitorKind::Http,
            MonitorConfig {
                expected_headers: Some(vec![
                    HttpHeaderMatch {
                        key: "x-state".into(),
                        value: "missing".into(),
                    },
                    HttpHeaderMatch {
                        key: "content-type".into(),
                        value: "json".into(),
                    },
                ]),
                header_match_mode: Some(HeaderMatchMode::Any),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new();
        observation.http_status = Some(200);
        observation
            .http_headers
            .insert("content-type".into(), "application/json".into());
        assert!(is_success(&monitor, &observation));
    }

    #[test]
    fn dns_expected_value_checks_answers() {
        let monitor = monitor(
            MonitorKind::Dns,
            MonitorConfig {
                expected_value: Some("1.1.1.1".into()),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new();
        observation.dns_answers = vec!["1.1.1.1".into()];
        assert!(is_success(&monitor, &observation));
    }

    #[test]
    fn ping_and_tcp_default_to_probe_success() {
        assert!(is_success(
            &monitor(MonitorKind::Ping, MonitorConfig::default()),
            &ProbeObservation::new()
        ));
        assert!(is_success(
            &monitor(MonitorKind::Tcp, MonitorConfig::default()),
            &ProbeObservation::new()
        ));
    }
}
