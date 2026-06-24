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
#[cfg(test)]
pub fn is_success(monitor: &Monitor, observation: &ProbeObservation) -> bool {
    failure_reason(monitor, observation).is_none()
}

/// 返回不满足成功条件的第一条可读原因。
pub fn failure_reason(monitor: &Monitor, observation: &ProbeObservation) -> Option<String> {
    match monitor.kind {
        MonitorKind::Http => {
            let Some(status) = observation.http_status else {
                return Some("未收到 HTTP 响应状态码".to_string());
            };

            if let Some(expected) = monitor.config.expected_status {
                if status != expected {
                    return Some(format!("HTTP 状态码为 {status}，期望 {expected}"));
                }
            } else {
                let min = monitor.config.expected_status_min.unwrap_or(200);
                let max = monitor.config.expected_status_max.unwrap_or(400);
                if status < min || status >= max {
                    let max_display = max.saturating_sub(1);
                    return Some(format!(
                        "HTTP 状态码为 {status}，不在期望范围 {min}-{max_display}"
                    ));
                }
            }

            if let Some(keyword) = &monitor.config.keyword {
                let Some(body) = observation.http_body.as_deref() else {
                    return Some("响应体未读取，无法匹配关键词".to_string());
                };
                let regex = match regex::Regex::new(keyword) {
                    Ok(regex) => regex,
                    Err(error) => {
                        return Some(format!("响应关键词正则无效：{error}"));
                    }
                };
                if !regex.is_match(body) {
                    return Some(format!("响应体未匹配关键词/正则：{keyword}"));
                };
            }

            headers_failure_reason(monitor, observation)
        }
        MonitorKind::Ping | MonitorKind::Tcp => None,
        MonitorKind::Dns => {
            if observation.dns_answers.is_empty() {
                return Some("DNS 未返回任何记录".to_string());
            }
            if let Some(expected) = &monitor.config.expected_value {
                if observation
                    .dns_answers
                    .iter()
                    .any(|value| value == expected)
                {
                    None
                } else {
                    Some(format!(
                        "DNS 结果未包含期望值 {expected}，实际为 {}",
                        observation.dns_answers.join(", ")
                    ))
                }
            } else {
                None
            }
        }
    }
}

fn headers_failure_reason(monitor: &Monitor, observation: &ProbeObservation) -> Option<String> {
    let rules = monitor
        .config
        .expected_headers
        .as_deref()
        .unwrap_or_default();
    if rules.is_empty() {
        return None;
    }

    let rule_matches = |key: &str, value: &str| -> Result<bool, String> {
        let normalized_key = key.to_ascii_lowercase();
        let Some(header_value) = observation.http_headers.get(&normalized_key) else {
            return Ok(false);
        };
        regex::Regex::new(value)
            .map(|regex| regex.is_match(header_value))
            .map_err(|error| format!("响应头 {key} 的正则无效：{error}"))
    };

    match monitor.config.header_match_mode {
        Some(HeaderMatchMode::Any) => {
            let mut errors = Vec::new();
            for rule in rules {
                match rule_matches(rule.key.trim(), rule.value.trim()) {
                    Ok(true) => return None,
                    Ok(false) => {}
                    Err(error) => errors.push(error),
                }
            }
            errors
                .into_iter()
                .next()
                .or_else(|| Some("没有任何响应头规则匹配".to_string()))
        }
        _ => {
            for rule in rules {
                let key = rule.key.trim();
                let expected = rule.value.trim();
                match rule_matches(key, expected) {
                    Ok(true) => {}
                    Ok(false) => {
                        let actual = observation
                            .http_headers
                            .get(&key.to_ascii_lowercase())
                            .map_or("未返回".to_string(), |value| format!("实际值：{value}"));
                        return Some(format!("响应头 {key} 未匹配 {expected}（{actual}）"));
                    }
                    Err(error) => return Some(error),
                }
            }
            None
        }
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
        assert_eq!(
            failure_reason(&monitor, &observation),
            Some("HTTP 状态码为 404，不在期望范围 200-399".to_string())
        );
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
    fn failure_reason_describes_dns_mismatch() {
        let monitor = monitor(
            MonitorKind::Dns,
            MonitorConfig {
                expected_value: Some("1.1.1.1".into()),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new();
        observation.dns_answers = vec!["8.8.8.8".into()];

        assert_eq!(
            failure_reason(&monitor, &observation),
            Some("DNS 结果未包含期望值 1.1.1.1，实际为 8.8.8.8".to_string())
        );
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
