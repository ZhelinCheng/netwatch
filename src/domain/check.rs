use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 单次探测结果状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Up,
    Down,
}

impl CheckStatus {
    /// 持久化到数据库时使用的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

impl From<&str> for CheckStatus {
    fn from(value: &str) -> Self {
        match value {
            "up" => Self::Up,
            _ => Self::Down,
        }
    }
}

/// 单次探测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    /// 数据库自增 ID；新结果写入前为空。
    pub id: Option<i64>,
    pub monitor_id: String,
    pub status: CheckStatus,
    /// 成功探测的耗时；失败时为空，错误原因写入 `error`。
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    /// 协议特定信息，例如 HTTP 状态码、DNS 解析结果或 ping 输出摘要。
    pub metadata: Value,
    pub checked_at: DateTime<Utc>,
}

impl CheckResult {
    /// 构造成功探测结果。
    pub fn up(monitor_id: String, latency_ms: u64, metadata: Value) -> Self {
        Self {
            id: None,
            monitor_id,
            status: CheckStatus::Up,
            latency_ms: Some(latency_ms),
            error: None,
            metadata,
            checked_at: Utc::now(),
        }
    }

    /// 构造失败探测结果。
    pub fn down(monitor_id: String, error: impl Into<String>, metadata: Value) -> Self {
        Self {
            id: None,
            monitor_id,
            status: CheckStatus::Down,
            latency_ms: None,
            error: Some(error.into()),
            metadata,
            checked_at: Utc::now(),
        }
    }
}

/// 面向 Dashboard 和详情页的聚合指标。
#[derive(Debug, Clone, Serialize)]
pub struct LatencyMetrics {
    pub total: usize,
    pub up: usize,
    pub down: usize,
    pub availability: f64,
    pub average_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<u64>,
}

impl LatencyMetrics {
    /// 基于最近一批探测结果计算可用率和延迟指标。
    pub fn from_results(results: &[CheckResult]) -> Self {
        let total = results.len();
        let up = results
            .iter()
            .filter(|result| result.status == CheckStatus::Up)
            .count();
        let down = total.saturating_sub(up);
        let availability = if total == 0 {
            0.0
        } else {
            (up as f64 / total as f64) * 100.0
        };

        let mut latencies: Vec<u64> = results
            .iter()
            .filter_map(|result| result.latency_ms)
            .collect();
        latencies.sort_unstable();

        let average_latency_ms = if latencies.is_empty() {
            None
        } else {
            Some(latencies.iter().sum::<u64>() as f64 / latencies.len() as f64)
        };
        let p95_latency_ms = percentile(&latencies, 0.95);

        Self {
            total,
            up,
            down,
            availability,
            average_latency_ms,
            p95_latency_ms,
        }
    }
}

fn percentile(values: &[u64], quantile: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let index = ((values.len() as f64 - 1.0) * quantile).ceil() as usize;
    values.get(index).copied()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn metrics_include_availability_and_p95() {
        let mut results = vec![
            CheckResult::up("m1".into(), 10, json!({})),
            CheckResult::up("m1".into(), 20, json!({})),
            CheckResult::up("m1".into(), 100, json!({})),
            CheckResult::down("m1".into(), "boom", json!({})),
        ];
        for result in &mut results {
            result.checked_at = Utc::now();
        }

        let metrics = LatencyMetrics::from_results(&results);
        assert_eq!(metrics.total, 4);
        assert_eq!(metrics.up, 3);
        assert_eq!(metrics.down, 1);
        assert_eq!(metrics.availability, 75.0);
        assert_eq!(metrics.p95_latency_ms, Some(100));
    }
}
