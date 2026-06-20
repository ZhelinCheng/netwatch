//! 按日历边界压缩探测结果。

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, FixedOffset, TimeZone, Timelike, Utc};

use crate::{
    domain::{
        check::{AggregateBucketSize, CheckAggregate, CheckResult, CheckStatus},
        monitor::Monitor,
    },
    state::AppState,
    storage::{aggregates, checks, monitors},
};

const LATENCY_BUCKET_BOUNDS_US: [u64; 10] = [
    50_000, 100_000, 200_000, 500_000, 1_000_000, 2_000_000, 5_000_000, 10_000_000, 30_000_000,
    60_000_000,
];
const RAW_RETENTION_HOURS: i64 = 25;

/// 执行一次历史数据保留清理。
pub async fn run(state: AppState) -> anyhow::Result<()> {
    let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
    let now = Utc::now();
    let today_start = local_day_start_utc(now, timezone);
    // 保留策略：原始点保留今天，分钟聚合保留 7 天，小时聚合保留 30 天。
    let minute_cutoff = today_start - Duration::days(7);
    let hour_cutoff = today_start - Duration::days(30);
    let raw_delete_cutoff = now - Duration::hours(RAW_RETENTION_HOURS);
    let monitors = monitors::list(state.pool()).await?;
    tracing::info!(
        monitor_count = monitors.len(),
        raw_delete_cutoff = %raw_delete_cutoff,
        minute_cutoff = %minute_cutoff,
        hour_cutoff = %hour_cutoff,
        "starting compaction"
    );

    for monitor in monitors {
        delete_raw_with_finished_day_rollup(
            &state,
            &monitor,
            today_start,
            raw_delete_cutoff,
            timezone,
        )
        .await?;
        aggregates::delete_older_than(
            state.pool(),
            monitor.id,
            AggregateBucketSize::Minute,
            minute_cutoff,
        )
        .await?;
        aggregates::delete_older_than(
            state.pool(),
            monitor.id,
            AggregateBucketSize::Hour,
            hour_cutoff,
        )
        .await?;
    }

    tracing::info!("compaction completed");
    Ok(())
}

/// 将配置值转换为固定时区偏移。
pub fn aggregation_offset(value: &str) -> FixedOffset {
    match value {
        "UTC" | "Etc/UTC" | "Z" => FixedOffset::east_opt(0).expect("valid UTC offset"),
        "Asia/Shanghai" | "Asia/Chongqing" | "PRC" => {
            FixedOffset::east_opt(8 * 60 * 60).expect("valid Shanghai offset")
        }
        other => parse_fixed_offset(other)
            .unwrap_or_else(|| FixedOffset::east_opt(0).expect("valid fallback offset")),
    }
}

/// 计算当前日历日零点对应的 UTC 时间。
pub fn local_day_start_utc(now: DateTime<Utc>, timezone: FixedOffset) -> DateTime<Utc> {
    let local_now = now.with_timezone(&timezone);
    let local_midnight = local_now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    timezone
        .from_local_datetime(&local_midnight)
        .single()
        .expect("fixed offset has one local datetime")
        .with_timezone(&Utc)
}

/// 计算当前本地小时整点对应的 UTC 时间。
pub fn local_hour_start_utc(now: DateTime<Utc>, timezone: FixedOffset) -> DateTime<Utc> {
    let local_now = now.with_timezone(&timezone);
    let local_hour = local_now
        .date_naive()
        .and_hms_opt(local_now.hour(), 0, 0)
        .expect("hour start is valid");
    timezone
        .from_local_datetime(&local_hour)
        .single()
        .expect("fixed offset has one local datetime")
        .with_timezone(&Utc)
}

async fn delete_raw_with_finished_day_rollup(
    state: &AppState,
    monitor: &Monitor,
    today_start: DateTime<Utc>,
    raw_delete_cutoff: DateTime<Utc>,
    timezone: FixedOffset,
) -> anyhow::Result<()> {
    // 只删除已经有 day 聚合覆盖的旧原始点，避免 rollup 失败时丢失精确数据。
    let raw_results =
        checks::list_for_monitor_between(state.pool(), monitor.id, monitor.created_at, today_start)
            .await?;
    let days = days_with_raw_results(raw_results, timezone);

    for (day_start, _) in days {
        let day_end = day_start + Duration::days(1);
        if day_start >= today_start {
            continue;
        }
        let mut tx = state.pool().begin().await?;
        let day_already_aggregated = !aggregates::list_for_monitor_day_tx(
            &mut tx,
            monitor.id,
            AggregateBucketSize::Day,
            day_start,
            day_end,
        )
        .await?
        .is_empty();

        if !day_already_aggregated {
            tx.commit().await?;
            continue;
        }

        let delete_until = day_end.min(raw_delete_cutoff);
        if delete_until > day_start {
            tracing::info!(
                monitor_id = monitor.id,
                day_start = %day_start,
                delete_until = %delete_until,
                "deleting raw check results covered by day rollup"
            );
            checks::delete_for_monitor_between_tx(&mut tx, monitor.id, day_start, delete_until)
                .await?;
        }
        tx.commit().await?;
    }

    Ok(())
}

/// 从原始探测点推导出需要重新聚合的本地日历日。
fn days_with_raw_results(
    results: Vec<CheckResult>,
    timezone: FixedOffset,
) -> BTreeMap<DateTime<Utc>, ()> {
    results
        .into_iter()
        .map(|result| (local_day_start_utc(result.checked_at, timezone), ()))
        .collect()
}

/// 将刚落库结果涉及的分钟桶重算为 minute 聚合。
pub async fn rollup_minutes_for_results_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    results: &[CheckResult],
) -> anyhow::Result<()> {
    let mut buckets = BTreeSet::new();
    for result in results {
        buckets.insert((result.monitor_id, utc_minute_start(result.checked_at)));
    }

    for (monitor_id, bucket_start) in buckets {
        let monitor = match monitors::get_tx(tx, monitor_id).await {
            Ok(monitor) => monitor,
            Err(crate::error::AppError::NotFound) => {
                tracing::debug!(
                    monitor_id,
                    "skipping minute rollup because monitor no longer exists"
                );
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        rollup_raw_window_tx(
            tx,
            &monitor,
            AggregateBucketSize::Minute,
            bucket_start,
            bucket_start + Duration::minutes(1),
            true,
        )
        .await?;
    }

    Ok(())
}

/// 每次 flush 后尝试生成上一完整小时的 hour 聚合。
pub async fn rollup_hour_if_due(state: &AppState) -> anyhow::Result<()> {
    let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
    let (hour_start, hour_end) =
        previous_finished_hour_window(Utc::now(), timezone, flush_grace(state));

    rollup_finished_window(state, AggregateBucketSize::Hour, hour_start, hour_end).await
}

/// 每次 flush 后尝试生成上一完整本地日的 day 聚合。
pub async fn rollup_day_if_due(state: &AppState) -> anyhow::Result<()> {
    let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
    let (day_start, day_end) =
        previous_finished_day_window(Utc::now(), timezone, flush_grace(state));

    rollup_finished_window(state, AggregateBucketSize::Day, day_start, day_end).await
}

async fn rollup_finished_window(
    state: &AppState,
    bucket_size: AggregateBucketSize,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> anyhow::Result<()> {
    let monitors = monitors::list(state.pool()).await?;
    tracing::debug!(
        bucket_size = bucket_size.as_str(),
        window_start = %window_start,
        window_end = %window_end,
        monitor_count = monitors.len(),
        "checking finished rollup window"
    );
    for monitor in monitors {
        if window_end <= monitor.created_at {
            continue;
        }

        let mut tx = state.pool().begin().await?;
        let exists = !aggregates::list_for_monitor_day_tx(
            &mut tx,
            monitor.id,
            bucket_size,
            window_start,
            window_end,
        )
        .await?
        .is_empty();
        if exists {
            tx.commit().await?;
            continue;
        }

        rollup_raw_window_tx(
            &mut tx,
            &monitor,
            bucket_size,
            window_start,
            window_end,
            false,
        )
        .await?;
        tx.commit().await?;
        tracing::info!(
            monitor_id = monitor.id,
            bucket_size = bucket_size.as_str(),
            window_start = %window_start,
            window_end = %window_end,
            "rollup window persisted"
        );
    }

    Ok(())
}

async fn rollup_raw_window_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    monitor: &Monitor,
    bucket_size: AggregateBucketSize,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    overwrite_existing: bool,
) -> anyhow::Result<()> {
    if !overwrite_existing {
        let exists = !aggregates::list_for_monitor_day_tx(
            tx,
            monitor.id,
            bucket_size,
            window_start,
            window_end,
        )
        .await?
        .is_empty();
        if exists {
            return Ok(());
        }
    }

    let raw_results =
        checks::list_for_monitor_between_tx(tx, monitor.id, window_start, window_end).await?;
    tracing::debug!(
        monitor_id = monitor.id,
        bucket_size = bucket_size.as_str(),
        window_start = %window_start,
        window_end = %window_end,
        raw_result_count = raw_results.len(),
        overwrite_existing = overwrite_existing,
        "rolling up raw check window"
    );
    let aggregate_points =
        aggregate_raw_window(monitor, bucket_size, window_start, window_end, &raw_results);
    for aggregate in aggregate_points {
        aggregates::upsert_tx(tx, &aggregate).await?;
    }

    Ok(())
}

/// 将某一天的原始探测点按指定粒度聚合。
#[cfg(test)]
pub fn aggregate_raw_day(
    monitor: &Monitor,
    bucket_size: AggregateBucketSize,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
    results: &[CheckResult],
) -> Vec<CheckAggregate> {
    aggregate_raw_window(monitor, bucket_size, day_start, day_end, results)
}

/// 将任意完整窗口内的原始探测点按指定粒度聚合。
pub fn aggregate_raw_window(
    monitor: &Monitor,
    bucket_size: AggregateBucketSize,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    results: &[CheckResult],
) -> Vec<CheckAggregate> {
    let mut grouped: BTreeMap<DateTime<Utc>, Vec<&CheckResult>> = BTreeMap::new();
    for result in results {
        let offset_seconds = result
            .checked_at
            .signed_duration_since(window_start)
            .num_seconds();
        if offset_seconds >= 0 {
            let bucket_seconds = bucket_size.seconds();
            // bucket_start 按窗口起点对齐，确保所有粒度都遵循配置时区的日历边界。
            let bucket_start = window_start
                + Duration::seconds((offset_seconds / bucket_seconds) * bucket_seconds);
            grouped.entry(bucket_start).or_default().push(result);
        }
    }

    let mut aggregates = Vec::new();
    let mut bucket_start = window_start;
    let step = Duration::seconds(bucket_size.seconds());
    while bucket_start < window_end {
        let bucket_end = (bucket_start + step).min(window_end);
        let bucket_results = grouped.remove(&bucket_start).unwrap_or_default();
        aggregates.push(aggregate_raw_bucket(
            monitor,
            bucket_size,
            bucket_start,
            bucket_end,
            &bucket_results,
        ));
        bucket_start = bucket_end;
    }

    aggregates
}

/// 聚合单个时间桶中的探测点。
fn aggregate_raw_bucket(
    monitor: &Monitor,
    bucket_size: AggregateBucketSize,
    bucket_start: DateTime<Utc>,
    bucket_end: DateTime<Utc>,
    results: &[&CheckResult],
) -> CheckAggregate {
    let success_count = results
        .iter()
        .filter(|result| result.status == CheckStatus::Success)
        .count() as u64;
    let failed_count = results
        .iter()
        .filter(|result| result.status == CheckStatus::Failed)
        .count() as u64;
    let mut latencies: Vec<u64> = results
        .iter()
        .filter(|result| result.status == CheckStatus::Success)
        .filter_map(|result| result.latency_us)
        .collect();
    latencies.sort_unstable();

    let expected = expected_points(monitor, bucket_start, bucket_end);
    let actual = success_count + failed_count;
    let latency_buckets = latency_buckets_from_values(&latencies);
    let now = Utc::now();

    CheckAggregate {
        id: None,
        monitor_id: monitor.id,
        bucket_size,
        bucket_start,
        bucket_end,
        success_count,
        failed_count,
        unknown_count: expected.saturating_sub(actual),
        latency_count: latencies.len() as u64,
        latency_sum_us: latencies.iter().sum(),
        min_latency_us: latencies.first().copied(),
        max_latency_us: latencies.last().copied(),
        p95_latency_us: percentile(&latencies, 0.95),
        latency_buckets,
        created_at: now,
        updated_at: now,
    }
}

/// 根据监控项间隔估算一个桶内本应产生多少探测点。
fn expected_points(
    monitor: &Monitor,
    bucket_start: DateTime<Utc>,
    bucket_end: DateTime<Utc>,
) -> u64 {
    let effective_start = bucket_start.max(monitor.created_at);
    if effective_start >= bucket_end {
        return 0;
    }
    let seconds = bucket_end
        .signed_duration_since(effective_start)
        .num_seconds()
        .max(0) as u64;
    seconds.div_ceil(monitor.interval_seconds)
}

/// 将延迟值落入固定边界直方图。
fn latency_buckets_from_values(values: &[u64]) -> Vec<u64> {
    let mut buckets = vec![0; LATENCY_BUCKET_BOUNDS_US.len() + 1];
    for value in values {
        let index = LATENCY_BUCKET_BOUNDS_US
            .iter()
            .position(|bound| value <= bound)
            .unwrap_or(LATENCY_BUCKET_BOUNDS_US.len());
        buckets[index] += 1;
    }
    buckets
}

fn utc_minute_start(value: DateTime<Utc>) -> DateTime<Utc> {
    let timestamp = value.timestamp();
    Utc.timestamp_opt(timestamp - timestamp.rem_euclid(60), 0)
        .single()
        .expect("valid minute timestamp")
}

fn flush_grace(state: &AppState) -> Duration {
    Duration::from_std(state.config().check_flush_interval)
        .unwrap_or_else(|_| Duration::seconds(60))
}

fn previous_finished_hour_window(
    now: DateTime<Utc>,
    timezone: FixedOffset,
    grace: Duration,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let hour_end = local_hour_start_utc(now - grace, timezone);
    (hour_end - Duration::hours(1), hour_end)
}

fn previous_finished_day_window(
    now: DateTime<Utc>,
    timezone: FixedOffset,
    grace: Duration,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let day_end = local_day_start_utc(now - grace, timezone);
    (day_end - Duration::days(1), day_end)
}

/// 从已排序延迟数组中取分位数。
fn percentile(values: &[u64], quantile: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let index = ((values.len() as f64 - 1.0) * quantile).ceil() as usize;
    values.get(index).copied()
}

/// 解析 `+08:00` / `-05:30` 这类固定时区偏移。
fn parse_fixed_offset(value: &str) -> Option<FixedOffset> {
    let sign = match value.as_bytes().first().copied()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let value = &value[1..];
    let (hours, minutes) = value.split_once(':')?;
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    FixedOffset::east_opt(sign * (hours * 60 * 60 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::domain::monitor::{MonitorConfig, MonitorKind};

    use super::*;

    fn monitor() -> Monitor {
        let created_at = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        Monitor {
            id: 1,
            name: "m1".into(),
            kind: MonitorKind::Http,
            target: "https://example.com".into(),
            config: MonitorConfig::default(),
            interval_seconds: 5,
            timeout_seconds: 1,
            enabled: true,
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn shanghai_day_start_uses_calendar_midnight() {
        let timezone = aggregation_offset("Asia/Shanghai");
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 16, 5, 0).unwrap();

        assert_eq!(
            local_day_start_utc(now, timezone),
            Utc.with_ymd_and_hms(2026, 6, 16, 16, 0, 0).unwrap()
        );
    }

    #[test]
    fn shanghai_hour_start_uses_calendar_hour() {
        let timezone = aggregation_offset("Asia/Shanghai");
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 16, 5, 0).unwrap();

        assert_eq!(
            local_hour_start_utc(now, timezone),
            Utc.with_ymd_and_hms(2026, 6, 16, 16, 0, 0).unwrap()
        );
    }

    #[test]
    fn rollup_windows_wait_for_flush_grace() {
        let timezone = aggregation_offset("UTC");
        let grace = Duration::seconds(60);
        let before_grace = Utc.with_ymd_and_hms(2026, 6, 16, 10, 0, 30).unwrap();
        let after_grace = Utc.with_ymd_and_hms(2026, 6, 16, 10, 1, 0).unwrap();

        assert_eq!(
            previous_finished_hour_window(before_grace, timezone, grace),
            (
                Utc.with_ymd_and_hms(2026, 6, 16, 8, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 6, 16, 9, 0, 0).unwrap(),
            )
        );
        assert_eq!(
            previous_finished_hour_window(after_grace, timezone, grace),
            (
                Utc.with_ymd_and_hms(2026, 6, 16, 9, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 6, 16, 10, 0, 0).unwrap(),
            )
        );
    }

    #[test]
    fn day_rollup_window_uses_local_midnight_after_grace() {
        let timezone = aggregation_offset("Asia/Shanghai");
        let grace = Duration::seconds(60);
        let now = Utc.with_ymd_and_hms(2026, 6, 16, 16, 1, 0).unwrap();

        assert_eq!(
            previous_finished_day_window(now, timezone, grace),
            (
                Utc.with_ymd_and_hms(2026, 6, 15, 16, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 6, 16, 16, 0, 0).unwrap(),
            )
        );
    }

    #[test]
    fn raw_minute_unknown_count_uses_interval() {
        let monitor = monitor();
        let day_start = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let day_end = day_start + Duration::minutes(1);
        let mut result = CheckResult::success(1, 42);
        result.checked_at = day_start;

        let aggregate = aggregate_raw_day(
            &monitor,
            AggregateBucketSize::Minute,
            day_start,
            day_end,
            &[result],
        )
        .into_iter()
        .next()
        .unwrap();

        assert_eq!(aggregate.success_count, 1);
        assert_eq!(aggregate.failed_count, 0);
        assert_eq!(aggregate.unknown_count, 11);
        assert_eq!(aggregate.latency_count, 1);
        assert_eq!(aggregate.latency_sum_us, 42);
        assert_eq!(aggregate.min_latency_us, Some(42));
        assert_eq!(aggregate.p95_latency_us, Some(42));
    }

    #[test]
    fn empty_raw_minute_becomes_unknown() {
        let monitor = monitor();
        let day_start = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let day_end = day_start + Duration::minutes(1);

        let aggregate = aggregate_raw_day(
            &monitor,
            AggregateBucketSize::Minute,
            day_start,
            day_end,
            &[],
        )
        .into_iter()
        .next()
        .unwrap();

        assert_eq!(aggregate.success_count, 0);
        assert_eq!(aggregate.failed_count, 0);
        assert_eq!(aggregate.unknown_count, 12);
    }

    #[test]
    fn raw_hour_uses_interval_expected_count() {
        let monitor = monitor();
        let day_start = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let day_end = day_start + Duration::hours(1);
        let merged =
            aggregate_raw_day(&monitor, AggregateBucketSize::Hour, day_start, day_end, &[]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].unknown_count, 720);
    }

    #[test]
    fn raw_hour_rollup_uses_exact_raw_latency_distribution() {
        let monitor = monitor();
        let hour_start = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let hour_end = hour_start + Duration::hours(1);
        let mut fast = CheckResult::success(1, 50);
        fast.checked_at = hour_start + Duration::minutes(1);
        let mut slow = CheckResult::success(1, 200);
        slow.checked_at = hour_start + Duration::minutes(2);
        let mut failed = CheckResult::failed(1, None);
        failed.checked_at = hour_start + Duration::minutes(3);

        let aggregate = aggregate_raw_window(
            &monitor,
            AggregateBucketSize::Hour,
            hour_start,
            hour_end,
            &[fast, slow, failed],
        )
        .into_iter()
        .next()
        .unwrap();

        assert_eq!(aggregate.success_count, 2);
        assert_eq!(aggregate.failed_count, 1);
        assert_eq!(aggregate.unknown_count, 717);
        assert_eq!(aggregate.latency_count, 2);
        assert_eq!(aggregate.latency_sum_us, 250);
        assert_eq!(aggregate.min_latency_us, Some(50));
        assert_eq!(aggregate.max_latency_us, Some(200));
        assert_eq!(aggregate.p95_latency_us, Some(200));
        assert_eq!(aggregate.latency_buckets[0], 2);
    }
}
