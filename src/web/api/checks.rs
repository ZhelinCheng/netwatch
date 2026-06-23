//! 探测结果查询 API。

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    domain::check::{
        AggregateBucketSize, AggregatePoint, CheckResult, CheckSeriesPoint, CheckStatus,
        LatencyMetrics,
    },
    error::AppError,
    scheduler::compact::{
        aggregate_raw_window, aggregation_offset, local_day_start_utc, local_hour_start_utc,
    },
    state::AppState,
    storage::{aggregates, checks, monitors},
};

#[derive(Debug, Deserialize, IntoParams)]
pub(crate) struct LimitQuery {
    /// 未指定时间范围时返回最近 N 条原始结果。
    limit: Option<i64>,
    /// 指定时间范围时的起点。
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    #[param(value_type = i64)]
    from: Option<DateTime<Utc>>,
    /// 指定时间范围时的终点。
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    #[param(value_type = i64)]
    to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ChecksResponse {
    /// 当前响应使用的分辨率：minute、hour 或 day；最近列表模式仍为 raw。
    resolution: String,
    /// 序列整体指标。
    metrics: LatencyMetrics,
    /// 最近列表模式返回原始点，时间范围查询返回单一粒度聚合点。
    results: Vec<CheckSeriesPoint>,
}

#[derive(Debug, Clone, Copy)]
struct TimeRange {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/monitors/{id}/checks", get(list))
}

#[utoipa::path(
    get,
    path = "/api/monitors/{id}/checks",
    operation_id = "list_monitor_checks",
    tag = "checks",
    params(
        ("id" = i64, Path, description = "监控项 ID"),
        LimitQuery
    ),
    responses(
        (status = 200, description = "探测结果时间序列", body = ChecksResponse),
        (status = 400, description = "请求参数无效"),
        (status = 404, description = "监控项不存在"),
        (status = 500, description = "服务端错误")
    )
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    Path(id): Path<i64>,
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

        let monitor = monitors::get(state.pool(), id).await?;
        let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
        let today_start = local_day_start_utc(Utc::now(), timezone);
        let minute_cutoff = today_start - Duration::days(7);
        let hour_cutoff = today_start - Duration::days(30);
        let query_range = TimeRange { from, to };
        let bucket_size = bucket_size_for_range(query_range, minute_cutoff, hour_cutoff);
        let results =
            list_aggregate_series(&state, &monitor, bucket_size, query_range, timezone).await?;

        (bucket_size.as_str().to_string(), results)
    } else {
        // 列表模式服务于详情页“最近结果”，只返回原始数据。
        let limit = query.limit.unwrap_or(100).clamp(1, 1000);
        let results = checks::list_for_monitor(state.pool(), id, limit)
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

/// 根据查询起点选择单一聚合粒度，避免同一个响应混合多种分辨率。
fn bucket_size_for_range(
    range: TimeRange,
    minute_cutoff: DateTime<Utc>,
    hour_cutoff: DateTime<Utc>,
) -> AggregateBucketSize {
    if range.from >= minute_cutoff {
        AggregateBucketSize::Minute
    } else if range.from >= hour_cutoff {
        AggregateBucketSize::Hour
    } else {
        AggregateBucketSize::Day
    }
}

/// 查询单一粒度的聚合序列，并用最近仍保留的 raw 数据生成同粒度临时桶覆盖未完成窗口。
async fn list_aggregate_series(
    state: &AppState,
    monitor: &crate::domain::monitor::Monitor,
    bucket_size: AggregateBucketSize,
    range: TimeRange,
    timezone: FixedOffset,
) -> Result<Vec<CheckSeriesPoint>, AppError> {
    let mut by_start = BTreeMap::new();
    let query_from = bucket_floor(range.from, bucket_size, timezone);
    for aggregate in aggregates::list_for_monitor_between(
        state.pool(),
        monitor.id,
        bucket_size,
        query_from,
        range.to,
    )
    .await?
    {
        by_start.insert(aggregate.bucket_start, aggregate);
    }

    let raw_cutoff = Utc::now() - Duration::hours(25);
    let dynamic_from = range.from.max(raw_cutoff);
    if dynamic_from <= range.to {
        let window_start = bucket_floor(dynamic_from, bucket_size, timezone);
        let window_end = bucket_ceil(range.to, bucket_size, timezone);
        let mut raw_results =
            checks::list_for_monitor_between(state.pool(), monitor.id, window_start, window_end)
                .await?;
        raw_results.extend(
            state
                .check_buffer()
                .list_for_monitor_between(monitor.id, window_start, window_end)
                .await,
        );

        for aggregate in
            aggregate_raw_window(monitor, bucket_size, window_start, window_end, &raw_results)
        {
            if aggregate.bucket_end > range.from && aggregate.bucket_start <= range.to {
                by_start.insert(aggregate.bucket_start, aggregate);
            }
        }
    }

    Ok(by_start
        .into_values()
        .filter(|aggregate| aggregate.bucket_end > range.from && aggregate.bucket_start <= range.to)
        .map(AggregatePoint::from)
        .map(CheckSeriesPoint::Aggregate)
        .collect())
}

fn bucket_floor(
    value: DateTime<Utc>,
    bucket_size: AggregateBucketSize,
    timezone: FixedOffset,
) -> DateTime<Utc> {
    match bucket_size {
        AggregateBucketSize::Minute => {
            let timestamp = value.timestamp();
            Utc.timestamp_opt(timestamp - timestamp.rem_euclid(60), 0)
                .single()
                .expect("valid minute timestamp")
        }
        AggregateBucketSize::Hour => local_hour_start_utc(value, timezone),
        AggregateBucketSize::Day => local_day_start_utc(value, timezone),
    }
}

fn bucket_ceil(
    value: DateTime<Utc>,
    bucket_size: AggregateBucketSize,
    timezone: FixedOffset,
) -> DateTime<Utc> {
    let floor = bucket_floor(value, bucket_size, timezone);
    if floor == value {
        floor
    } else {
        floor + Duration::seconds(bucket_size.seconds())
    }
}

/// 从原始点或聚合点序列中计算总览指标。
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

/// 序列指标的临时汇总器。
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

    use crate::domain::check::CheckResult;

    use super::*;

    #[test]
    fn range_bucket_selection_and_metrics_cover_aggregate_series() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            bucket_size_for_range(
                TimeRange {
                    from,
                    to: from + Duration::hours(1),
                },
                from - Duration::days(1),
                from - Duration::days(30),
            ),
            AggregateBucketSize::Minute
        );
        assert_eq!(
            bucket_size_for_range(
                TimeRange {
                    from: from - Duration::days(10),
                    to: from,
                },
                from - Duration::days(7),
                from - Duration::days(30),
            ),
            AggregateBucketSize::Hour
        );
        assert_eq!(
            bucket_size_for_range(
                TimeRange {
                    from: from - Duration::days(40),
                    to: from,
                },
                from - Duration::days(7),
                from - Duration::days(30),
            ),
            AggregateBucketSize::Day
        );

        let aggregate = AggregatePoint {
            monitor_id: 1,
            bucket_size: AggregateBucketSize::Minute,
            bucket_start: from + Duration::minutes(1),
            bucket_end: from + Duration::minutes(2),
            success_count: 2,
            failed_count: 1,
            unknown_count: 1,
            availability: 0.0,
            avg_latency_us: Some(20.0),
            p95_latency_us: Some(30),
            min_latency_us: Some(10),
            max_latency_us: Some(30),
            latency_count: 2,
            latency_sum_us: 40,
        };
        let series = vec![CheckSeriesPoint::Aggregate(aggregate)];

        let metrics = metrics_from_series(&series);

        assert_eq!(metrics.total, 4);
        assert_eq!(metrics.success, 2);
        assert_eq!(metrics.failed, 1);
        assert_eq!(metrics.unknown, 1);
        assert_eq!(metrics.average_latency_us, Some(20.0));
        assert_eq!(metrics.p95_latency_us, Some(30));
    }

    #[tokio::test]
    async fn list_handler_supports_recent_and_range_queries() {
        let state = crate::test_support::state("api-checks").await;
        let monitor = monitors::insert(
            state.pool(),
            &crate::test_support::monitor(crate::domain::monitor::MonitorKind::Http),
        )
        .await
        .unwrap();
        let mut result = CheckResult::success(monitor.id, 10);
        result.checked_at = Utc::now();
        let mut tx = state.pool().begin().await.unwrap();
        checks::insert_many_tx(&mut tx, &[result.clone()])
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let Json(recent) = list(
            State(state.clone()),
            Path(monitor.id),
            Query(LimitQuery {
                limit: Some(5),
                from: None,
                to: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(recent.resolution, "raw");
        assert_eq!(recent.results.len(), 1);

        let Json(ranged) = list(
            State(state.clone()),
            Path(monitor.id),
            Query(LimitQuery {
                limit: None,
                from: Some(result.checked_at),
                to: Some(result.checked_at),
            }),
        )
        .await
        .unwrap();
        assert_eq!(ranged.resolution, "minute");
        assert!(!ranged.results.is_empty());
        assert!(
            ranged
                .results
                .iter()
                .all(|point| matches!(point, CheckSeriesPoint::Aggregate(_)))
        );

        let error = list(
            State(state),
            Path(monitor.id),
            Query(LimitQuery {
                limit: None,
                from: Some(result.checked_at + Duration::seconds(1)),
                to: Some(result.checked_at),
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::BadRequest(_)));
    }
}
