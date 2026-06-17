use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 单次探测结果状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Success,
    Failed,
    Unknown,
}

impl CheckStatus {
    /// 持久化到数据库时使用的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

impl From<&str> for CheckStatus {
    fn from(value: &str) -> Self {
        match value {
            "success" | "up" => Self::Success,
            "unknown" => Self::Unknown,
            _ => Self::Failed,
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
    /// 成功探测的耗时；失败或 unknown 时可以为空。
    pub latency_us: Option<u64>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub checked_at: DateTime<Utc>,
}

impl CheckResult {
    /// 构造成功探测结果。
    pub fn success(monitor_id: String, latency_us: u64) -> Self {
        Self {
            id: None,
            monitor_id,
            status: CheckStatus::Success,
            latency_us: Some(latency_us),
            checked_at: Utc::now(),
        }
    }

    /// 构造失败探测结果。
    pub fn failed(monitor_id: String, latency_us: Option<u64>) -> Self {
        Self {
            id: None,
            monitor_id,
            status: CheckStatus::Failed,
            latency_us,
            checked_at: Utc::now(),
        }
    }

    /// 构造仅用于 API 响应的 unknown 虚拟点。
    pub fn unknown(monitor_id: String, checked_at: DateTime<Utc>) -> Self {
        Self {
            id: None,
            monitor_id,
            status: CheckStatus::Unknown,
            latency_us: None,
            checked_at,
        }
    }
}

/// 面向 Dashboard 和详情页的聚合指标。
#[derive(Debug, Clone, Serialize)]
pub struct LatencyMetrics {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub unknown: usize,
    pub availability: f64,
    pub average_latency_us: Option<f64>,
    pub p95_latency_us: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateBucketSize {
    /// 分钟级聚合，用于最近 7 天的细粒度趋势。
    Minute,
    /// 小时级聚合，用于 7 到 30 天的中期趋势。
    Hour,
    /// 天级聚合，用于 30 天以前的长期趋势。
    Day,
}

impl AggregateBucketSize {
    /// 持久化到数据库时使用的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }

    /// 当前聚合桶覆盖的秒数。
    pub fn seconds(&self) -> i64 {
        match self {
            Self::Minute => 60,
            Self::Hour => 60 * 60,
            Self::Day => 24 * 60 * 60,
        }
    }
}

impl From<&str> for AggregateBucketSize {
    fn from(value: &str) -> Self {
        match value {
            "hour" => Self::Hour,
            "day" => Self::Day,
            _ => Self::Minute,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAggregate {
    /// 数据库自增 ID；聚合 upsert 前为空。
    pub id: Option<i64>,
    pub monitor_id: String,
    pub bucket_size: AggregateBucketSize,
    /// 聚合桶左闭区间起点。
    #[serde(with = "chrono::serde::ts_seconds")]
    pub bucket_start: DateTime<Utc>,
    /// 聚合桶右开区间终点。
    #[serde(with = "chrono::serde::ts_seconds")]
    pub bucket_end: DateTime<Utc>,
    pub success_count: u64,
    pub failed_count: u64,
    /// 预期探测点里缺失的数量，用于表达调度中断或尚未写入的空洞。
    pub unknown_count: u64,
    pub latency_count: u64,
    pub latency_sum_us: u64,
    pub min_latency_us: Option<u64>,
    pub max_latency_us: Option<u64>,
    pub p95_latency_us: Option<u64>,
    /// 固定边界的延迟直方图，便于后续展示分布。
    pub latency_buckets: Vec<u64>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

impl CheckAggregate {
    /// 只用 success/failed 计算可用率，unknown 表示缺测而不是失败。
    pub fn availability(&self) -> f64 {
        let measured = self.success_count + self.failed_count;
        if measured == 0 {
            0.0
        } else {
            (self.success_count as f64 / measured as f64) * 100.0
        }
    }

    /// 聚合桶内成功探测的平均延迟。
    pub fn average_latency_us(&self) -> Option<f64> {
        if self.latency_count == 0 {
            None
        } else {
            Some(self.latency_sum_us as f64 / self.latency_count as f64)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckSeriesPoint {
    /// 最近一天直接返回原始探测点。
    Raw(CheckResult),
    /// 更早的数据返回预计算聚合点，避免长时间范围查询过大。
    Aggregate(AggregatePoint),
}

/// 对外 API 返回的聚合序列点。
#[derive(Debug, Clone, Serialize)]
pub struct AggregatePoint {
    pub monitor_id: String,
    pub bucket_size: AggregateBucketSize,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub bucket_start: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub bucket_end: DateTime<Utc>,
    pub success_count: u64,
    pub failed_count: u64,
    pub unknown_count: u64,
    pub availability: f64,
    pub avg_latency_us: Option<f64>,
    pub p95_latency_us: Option<u64>,
    pub min_latency_us: Option<u64>,
    pub max_latency_us: Option<u64>,
    /// 内部汇总指标使用，不暴露给前端响应。
    #[serde(skip_serializing)]
    pub latency_count: u64,
    /// 内部汇总指标使用，不暴露给前端响应。
    #[serde(skip_serializing)]
    pub latency_sum_us: u64,
}

impl From<CheckAggregate> for AggregatePoint {
    fn from(aggregate: CheckAggregate) -> Self {
        let availability = aggregate.availability();
        let avg_latency_us = aggregate.average_latency_us();
        Self {
            monitor_id: aggregate.monitor_id,
            bucket_size: aggregate.bucket_size,
            bucket_start: aggregate.bucket_start,
            bucket_end: aggregate.bucket_end,
            success_count: aggregate.success_count,
            failed_count: aggregate.failed_count,
            unknown_count: aggregate.unknown_count,
            availability,
            avg_latency_us,
            p95_latency_us: aggregate.p95_latency_us,
            min_latency_us: aggregate.min_latency_us,
            max_latency_us: aggregate.max_latency_us,
            latency_count: aggregate.latency_count,
            latency_sum_us: aggregate.latency_sum_us,
        }
    }
}

#[cfg(test)]
impl LatencyMetrics {
    /// 基于最近一批探测结果计算可用率和延迟指标。
    pub fn from_results(results: &[CheckResult]) -> Self {
        let total = results.len();
        let success = results
            .iter()
            .filter(|result| result.status == CheckStatus::Success)
            .count();
        let failed = results
            .iter()
            .filter(|result| result.status == CheckStatus::Failed)
            .count();
        let unknown = results
            .iter()
            .filter(|result| result.status == CheckStatus::Unknown)
            .count();
        let measured = success + failed;
        let availability = if measured == 0 {
            0.0
        } else {
            (success as f64 / measured as f64) * 100.0
        };

        let mut latencies: Vec<u64> = results
            .iter()
            .filter(|result| result.status == CheckStatus::Success)
            .filter_map(|result| result.latency_us)
            .collect();
        latencies.sort_unstable();

        let average_latency_us = if latencies.is_empty() {
            None
        } else {
            Some(latencies.iter().sum::<u64>() as f64 / latencies.len() as f64)
        };
        let p95_latency_us = percentile(&latencies, 0.95);

        Self {
            total,
            success,
            failed,
            unknown,
            availability,
            average_latency_us,
            p95_latency_us,
        }
    }
}

#[cfg(test)]
fn percentile(values: &[u64], quantile: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    // values 调用方已排序，这里只按目标分位数取上界位置。
    let index = ((values.len() as f64 - 1.0) * quantile).ceil() as usize;
    values.get(index).copied()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    #[test]
    fn metrics_include_availability_and_p95() {
        let mut results = vec![
            CheckResult::success("m1".into(), 10),
            CheckResult::success("m1".into(), 20),
            CheckResult::success("m1".into(), 100),
            CheckResult::failed("m1".into(), None),
            CheckResult::unknown("m1".into(), Utc::now()),
        ];
        for result in &mut results {
            result.checked_at = Utc::now();
        }

        let metrics = LatencyMetrics::from_results(&results);
        assert_eq!(metrics.total, 5);
        assert_eq!(metrics.success, 3);
        assert_eq!(metrics.failed, 1);
        assert_eq!(metrics.unknown, 1);
        assert_eq!(metrics.availability, 75.0);
        assert_eq!(metrics.average_latency_us, Some(130.0 / 3.0));
        assert_eq!(metrics.p95_latency_us, Some(100));
    }

    #[test]
    fn status_accepts_old_and_new_strings() {
        assert_eq!(CheckStatus::from("success"), CheckStatus::Success);
        assert_eq!(CheckStatus::from("up"), CheckStatus::Success);
        assert_eq!(CheckStatus::from("failed"), CheckStatus::Failed);
        assert_eq!(CheckStatus::from("down"), CheckStatus::Failed);
        assert_eq!(CheckStatus::from("unknown"), CheckStatus::Unknown);
    }

    #[test]
    fn check_result_serializes_time_as_timestamp_seconds() {
        let mut result = CheckResult::success("m1".into(), 10);
        result.checked_at = Utc.with_ymd_and_hms(2026, 6, 17, 8, 9, 10).unwrap();

        let value = serde_json::to_value(&result).unwrap();

        assert_eq!(value["checked_at"], json!(1_781_683_750_i64));
        assert_eq!(value["latency_us"], json!(10));
        assert!(value.get("latency_ms").is_none());
    }
}
