//! 探测结果查询 API。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::check::{
        AggregateBucketSize, AggregatePoint, CheckResult, CheckSeriesPoint, CheckStatus,
        LatencyMetrics,
    },
    error::AppError,
    scheduler::compact::{aggregation_offset, local_day_start_utc},
    state::AppState,
    storage::{aggregates, checks, monitors},
};

#[derive(Debug, Deserialize)]
struct LimitQuery {
    /// 未指定时间范围时返回最近 N 条原始结果。
    limit: Option<i64>,
    /// 指定时间范围时的起点。
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    from: Option<DateTime<Utc>>,
    /// 指定时间范围时的终点。
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct ChecksResponse {
    /// 当前响应使用的分辨率：raw、minute、hour、day 或 mixed。
    resolution: String,
    /// 序列整体指标。
    metrics: LatencyMetrics,
    /// 原始点和聚合点混合后的时间序列。
    results: Vec<CheckSeriesPoint>,
}

#[derive(Debug, Clone, Copy)]
struct TimeRange {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

impl TimeRange {
    /// 计算两个时间范围的交集，空区间直接跳过。
    fn overlap(self, other: Self) -> Option<Self> {
        let from = self.from.max(other.from);
        let to = self.to.min(other.to);
        (from < to).then_some(Self { from, to })
    }
}

#[derive(Default)]
struct SeriesOutput {
    series: Vec<CheckSeriesPoint>,
    resolutions: Vec<&'static str>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/monitors/:id/checks", get(list))
}

async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<ChecksResponse>, AppError> {
    let (resolution, results) = if query.from.is_some() || query.to.is_some() {
        // 时间范围查询要求 from/to 成对出现，避免服务端猜测默认窗口。
        let from = query
            .from
            .ok_or_else(|| AppError::BadRequest("from is required when to is set".to_string()))?;
        let to = query
            .to
            .ok_or_else(|| AppError::BadRequest("to is required when from is set".to_string()))?;
        if from > to {
            return Err(AppError::BadRequest(
                "from must be earlier than or equal to to".to_string(),
            ));
        }

        let monitor = monitors::get(state.pool(), &id).await?;
        let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
        let today_start = local_day_start_utc(Utc::now(), timezone);
        let minute_cutoff = today_start - Duration::days(7);
        let hour_cutoff = today_start - Duration::days(30);
        let query_range = TimeRange { from, to };
        let mut output = SeriesOutput::default();

        // 按保留策略分段查询：越旧的数据粒度越粗，最近一天返回原始点。
        append_aggregate_segment(
            state.pool(),
            &monitor.id,
            query_range,
            TimeRange {
                from,
                to: hour_cutoff,
            },
            AggregateBucketSize::Day,
            &mut output,
        )
        .await?;
        append_aggregate_segment(
            state.pool(),
            &monitor.id,
            query_range,
            TimeRange {
                from: hour_cutoff,
                to: minute_cutoff,
            },
            AggregateBucketSize::Hour,
            &mut output,
        )
        .await?;
        append_aggregate_segment(
            state.pool(),
            &monitor.id,
            query_range,
            TimeRange {
                from: minute_cutoff,
                to: today_start,
            },
            AggregateBucketSize::Minute,
            &mut output,
        )
        .await?;
        append_raw_segment(
            &state,
            &monitor.id,
            monitor.interval_seconds,
            query_range,
            today_start,
            &mut output,
        )
        .await?;

        output.series.sort_by_key(series_time);
        (resolution_label(&output.resolutions), output.series)
    } else {
        // 列表模式服务于详情页“最近结果”，只返回原始数据。
        let limit = query.limit.unwrap_or(100).clamp(1, 1000);
        let results = checks::list_for_monitor(state.pool(), &id, limit)
            .await?
            .into_iter()
            .map(CheckSeriesPoint::Raw)
            .collect();
        ("raw".to_string(), results)
    };
    let metrics = metrics_from_series(&results);

    Ok(Json(ChecksResponse {
        resolution,
        metrics,
        results,
    }))
}

/// 查询某一个聚合保留区间，并追加到输出序列。
async fn append_aggregate_segment(
    pool: &sqlx::SqlitePool,
    monitor_id: &str,
    query_range: TimeRange,
    segment_range: TimeRange,
    bucket_size: AggregateBucketSize,
    output: &mut SeriesOutput,
) -> Result<(), AppError> {
    let Some(range) = query_range.overlap(segment_range) else {
        return Ok(());
    };

    let aggregates =
        aggregates::list_for_monitor_between(pool, monitor_id, bucket_size, range.from, range.to)
            .await?;
    if !aggregates.is_empty() {
        output.resolutions.push(bucket_size.as_str());
    }
    output.series.extend(
        aggregates
            .into_iter()
            .map(AggregatePoint::from)
            .map(CheckSeriesPoint::Aggregate),
    );

    Ok(())
}

/// 查询最近一天的原始结果，并把尚未 flush 的缓冲结果合并进来。
async fn append_raw_segment(
    state: &AppState,
    monitor_id: &str,
    interval_seconds: u64,
    query_range: TimeRange,
    today_start: DateTime<Utc>,
    output: &mut SeriesOutput,
) -> Result<(), AppError> {
    let from = query_range.from.max(today_start);
    let to = query_range.to;
    if from > to {
        return Ok(());
    }

    let mut real_results =
        checks::list_for_monitor_between(state.pool(), monitor_id, from, to).await?;
    real_results.extend(
        state
            .check_buffer()
            .list_for_monitor_between(monitor_id, from, to)
            .await,
    );
    real_results.sort_by_key(|result| result.checked_at);
    let results = fill_unknown_points(monitor_id, interval_seconds, from, to, real_results)?;
    if !results.is_empty() {
        output.resolutions.push("raw");
    }
    output
        .series
        .extend(results.into_iter().map(CheckSeriesPoint::Raw));

    Ok(())
}

/// 按监控项间隔填充缺失的 unknown 点，让图表能表现采集空洞。
fn fill_unknown_points(
    monitor_id: &str,
    interval_seconds: u64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    mut results: Vec<CheckResult>,
) -> Result<Vec<CheckResult>, AppError> {
    let interval = Duration::seconds(interval_seconds as i64);
    let tolerance = Duration::milliseconds(((interval_seconds as i64) * 1000 / 2).max(1));
    let mut expected_at = from;
    let mut expected_count = 0usize;

    while expected_at <= to {
        expected_count += 1;
        if expected_count > 1000 {
            return Err(AppError::BadRequest(
                "time range produces more than 1000 expected points".to_string(),
            ));
        }

        // 允许半个 interval 的抖动，避免调度 tick 和网络耗时造成重复补点。
        let has_real_result = results.iter().any(|result| {
            let distance = result
                .checked_at
                .signed_duration_since(expected_at)
                .num_milliseconds()
                .abs();
            distance <= tolerance.num_milliseconds()
        });
        if !has_real_result {
            results.push(CheckResult::unknown(monitor_id.to_string(), expected_at));
        }

        expected_at += interval;
    }

    results.sort_by_key(|result| result.checked_at);
    Ok(results)
}

/// 返回序列点用于排序的时间。
fn series_time(point: &CheckSeriesPoint) -> DateTime<Utc> {
    match point {
        CheckSeriesPoint::Raw(result) => result.checked_at,
        CheckSeriesPoint::Aggregate(aggregate) => aggregate.bucket_start,
    }
}

/// 生成响应中的分辨率标签。
fn resolution_label(resolutions: &[&str]) -> String {
    let mut unique = resolutions.to_vec();
    unique.sort_unstable();
    unique.dedup();
    match unique.as_slice() {
        [] => "raw".to_string(),
        [resolution] => (*resolution).to_string(),
        _ => "mixed".to_string(),
    }
}

/// 从原始点和聚合点混合序列中计算总览指标。
fn metrics_from_series(series: &[CheckSeriesPoint]) -> LatencyMetrics {
    let mut totals = AggregateTotals::default();

    for point in series {
        match point {
            CheckSeriesPoint::Raw(result) => totals.add_raw(result),
            CheckSeriesPoint::Aggregate(aggregate) => totals.add_aggregate(aggregate),
        }
    }

    totals.into_metrics()
}

/// 混合序列的临时汇总器。
#[derive(Default)]
struct AggregateTotals {
    total: usize,
    success: usize,
    failed: usize,
    unknown: usize,
    latency_count: u64,
    latency_sum_us: u64,
    p95_latency_us: Option<u64>,
    raw_latencies: Vec<u64>,
}

impl AggregateTotals {
    /// 累加一个原始探测点。
    fn add_raw(&mut self, result: &CheckResult) {
        self.total += 1;
        match result.status {
            CheckStatus::Success => {
                self.success += 1;
                if let Some(latency_us) = result.latency_us {
                    self.latency_count += 1;
                    self.latency_sum_us += latency_us;
                    self.raw_latencies.push(latency_us);
                }
            }
            CheckStatus::Failed => self.failed += 1,
            CheckStatus::Unknown => self.unknown += 1,
        }
    }

    /// 累加一个聚合点。
    fn add_aggregate(&mut self, aggregate: &AggregatePoint) {
        self.success += aggregate.success_count as usize;
        self.failed += aggregate.failed_count as usize;
        self.unknown += aggregate.unknown_count as usize;
        self.total = self.success + self.failed + self.unknown;
        self.latency_count += aggregate.latency_count;
        self.latency_sum_us += aggregate.latency_sum_us;
        self.p95_latency_us = self.p95_latency_us.max(aggregate.p95_latency_us);
    }

    /// 转换为 API 使用的指标结构。
    fn into_metrics(self) -> LatencyMetrics {
        let total = self.total;
        let success = self.success;
        let failed = self.failed;
        let unknown = self.unknown;
        let measured = success + failed;
        let availability = if measured == 0 {
            0.0
        } else {
            (success as f64 / measured as f64) * 100.0
        };
        let average_latency_us = if self.latency_count == 0 {
            None
        } else {
            Some(self.latency_sum_us as f64 / self.latency_count as f64)
        };

        LatencyMetrics {
            total,
            success,
            failed,
            unknown,
            availability,
            average_latency_us,
            p95_latency_us: self
                .p95_latency_us
                .max(percentile(&self.raw_latencies, 0.95)),
        }
    }
}

/// 对原始延迟值计算分位数。
fn percentile(values: &[u64], quantile: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() as f64 - 1.0) * quantile).ceil() as usize;
    values.get(index).copied()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::domain::check::CheckStatus;

    use super::*;

    #[test]
    fn fills_missing_points_with_unknown() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 10).unwrap();
        let mut result = CheckResult::success("m1".into(), 10);
        result.checked_at = from;

        let filled = fill_unknown_points("m1", 5, from, to, vec![result]).unwrap();

        assert_eq!(filled.len(), 3);
        assert_eq!(filled[0].status, CheckStatus::Success);
        assert_eq!(filled[1].status, CheckStatus::Unknown);
        assert_eq!(filled[2].status, CheckStatus::Unknown);
    }

    #[test]
    fn tolerance_prevents_duplicate_unknown() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = from;
        let mut result = CheckResult::success("m1".into(), 10);
        result.checked_at = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 1).unwrap();

        let filled = fill_unknown_points("m1", 5, from, to, vec![result]).unwrap();

        assert_eq!(filled.len(), 1);
        assert_eq!(filled[0].status, CheckStatus::Success);
    }
}
