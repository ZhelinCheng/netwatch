use std::collections::HashMap;

use crate::domain::monitor::{Monitor, MonitorKind, SuccessRule};

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
    /// 本次探测总耗时。
    pub latency_us: u64,
}

impl ProbeObservation {
    /// 用统一延迟值初始化观测对象，其它字段由具体探测器补充。
    pub fn new(latency_us: u64) -> Self {
        Self {
            latency_us,
            ..Self::default()
        }
    }
}

/// 判断一次探测是否满足成功条件。
pub fn is_success(monitor: &Monitor, observation: &ProbeObservation) -> bool {
    if monitor.config.has_success_rules() {
        // 一旦配置了自定义规则，就要求所有规则同时满足，协议默认规则不再参与。
        return monitor
            .config
            .success_rules
            .as_deref()
            .unwrap_or_default()
            .iter()
            .all(|rule| rule_matches(rule, observation));
    }

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
                body.contains(keyword)
            } else {
                true
            }
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

/// 执行单条自定义成功规则。
fn rule_matches(rule: &SuccessRule, observation: &ProbeObservation) -> bool {
    match rule {
        SuccessRule::HttpStatus { op, value } => observation
            .http_status
            .is_some_and(|status| op.matches_u64(status as u64, *value as u64)),
        SuccessRule::HttpBody { op, value } => observation
            .http_body
            .as_deref()
            .is_some_and(|body| op.matches(body, value)),
        SuccessRule::HttpHeader { key, op, value } => {
            let normalized_key = key.to_ascii_lowercase();
            observation
                .http_headers
                .get(&normalized_key)
                .is_some_and(|header_value| op.matches(header_value, value))
        }
        SuccessRule::DnsAnswer { op, value } => {
            op.matches_any(observation.dns_answers.iter().map(String::as_str), value)
        }
        SuccessRule::Latency { op, value_us } => op.matches_u64(observation.latency_us, *value_us),
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::monitor::{
        CompareOp, Monitor, MonitorConfig, MonitorKind, SuccessRule, TextOp,
    };

    use super::*;

    fn monitor(kind: MonitorKind, config: MonitorConfig) -> Monitor {
        let now = chrono::Utc::now();
        Monitor {
            id: "m1".into(),
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
        let mut observation = ProbeObservation::new(10);
        observation.http_status = Some(302);
        assert!(is_success(&monitor, &observation));

        observation.http_status = Some(404);
        assert!(!is_success(&monitor, &observation));
    }

    #[test]
    fn http_rules_check_body_and_headers() {
        let monitor = monitor(
            MonitorKind::Http,
            MonitorConfig {
                success_rules: Some(vec![
                    SuccessRule::HttpBody {
                        op: TextOp::Contains,
                        value: "ok".into(),
                    },
                    SuccessRule::HttpHeader {
                        key: "x-state".into(),
                        op: TextOp::Equals,
                        value: "ready".into(),
                    },
                ]),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new(10);
        observation.http_body = Some("all ok".into());
        observation
            .http_headers
            .insert("x-state".into(), "ready".into());
        assert!(is_success(&monitor, &observation));
    }

    #[test]
    fn dns_rules_check_answers() {
        let monitor = monitor(
            MonitorKind::Dns,
            MonitorConfig {
                success_rules: Some(vec![SuccessRule::DnsAnswer {
                    op: TextOp::Equals,
                    value: "1.1.1.1".into(),
                }]),
                ..MonitorConfig::default()
            },
        );
        let mut observation = ProbeObservation::new(10);
        observation.dns_answers = vec!["1.1.1.1".into()];
        assert!(is_success(&monitor, &observation));
    }

    #[test]
    fn latency_rule_is_supported() {
        let monitor = monitor(
            MonitorKind::Http,
            MonitorConfig {
                success_rules: Some(vec![SuccessRule::Latency {
                    op: CompareOp::Lt,
                    value_us: 100,
                }]),
                ..MonitorConfig::default()
            },
        );
        assert!(is_success(&monitor, &ProbeObservation::new(50)));
        assert!(!is_success(&monitor, &ProbeObservation::new(150)));
    }

    #[test]
    fn ping_and_tcp_default_to_probe_success() {
        assert!(is_success(
            &monitor(MonitorKind::Ping, MonitorConfig::default()),
            &ProbeObservation::new(10)
        ));
        assert!(is_success(
            &monitor(MonitorKind::Tcp, MonitorConfig::default()),
            &ProbeObservation::new(10)
        ));
    }
}
