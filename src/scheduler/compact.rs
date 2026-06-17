//! 按日历边界压缩探测结果。

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};

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

/// 执行一次历史数据压缩。
pub async fn run(state: AppState) -> anyhow::Result<()> {
    // 压缩前先把内存缓冲写库，避免刚产生的探测点被遗漏在聚合之外。
    super::flush::run(state.clone()).await?;

    let timezone = aggregation_offset(state.config().aggregation_timezone.as_str());
    let now = Utc::now();
    let today_start = local_day_start_utc(Utc::now(), timezone);
    // 保留策略：原始点保留今天，分钟聚合保留 7 天，小时聚合保留 30 天。
    let minute_cutoff = today_start - Duration::days(7);
    let hour_cutoff = today_start - Duration::days(30);
    let raw_delete_cutoff = now - Duration::hours(24);
    let monitors = monitors::list(state.pool()).await?;

    for monitor in monitors {
        compact_raw_days(&state, &monitor, today_start, raw_delete_cutoff, timezone).await?;
        aggregates::delete_older_than(
            state.pool(),
            &monitor.id,
            AggregateBucketSize::Minute,
            minute_cutoff,
        )
        .await?;
        aggregates::delete_older_than(
            state.pool(),
            &monitor.id,
            AggregateBucketSize::Hour,
            hour_cutoff,
        )
        .await?;
    }

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

async fn compact_raw_days(
    state: &AppState,
    monitor: &Monitor,
    today_start: DateTime<Utc>,
    raw_delete_cutoff: DateTime<Utc>,
    timezone: FixedOffset,
) -> anyhow::Result<()> {
    // 只压缩今天以前的完整日历日，今天仍保留原始点供实时图表使用。
    let raw_results = checks::list_for_monitor_between(
        state.pool(),
        &monitor.id,
        monitor.created_at,
        today_start,
    )
    .await?;
    let mut days = days_with_raw_results(raw_results, timezone);
    let yesterday_start = today_start - Duration::days(1);
    if yesterday_start + Duration::days(1) > monitor.created_at {
        // 即使昨天没有原始点，也生成 unknown 聚合，保留“应该探测但缺失”的信息。
        days.insert(yesterday_start, ());
    }

    for (day_start, _) in days {
        let day_end = day_start + Duration::days(1);
        let mut tx = state.pool().begin().await?;
        let day_already_aggregated = !aggregates::list_for_monitor_day_tx(
            &mut tx,
            &monitor.id,
            AggregateBucketSize::Day,
            day_start,
            day_end,
        )
        .await?
        .is_empty();

        if !day_already_aggregated {
            // 同一事务内完成聚合写入和原始数据清理，避免中间状态被 API 读到。
            let raw_results =
                checks::list_for_monitor_between_tx(&mut tx, &monitor.id, day_start, day_end)
                    .await?;
            let minute_aggregates = aggregate_raw_day(
                monitor,
                AggregateBucketSize::Minute,
                day_start,
                day_end,
                &raw_results,
            );
            let hour_aggregates = aggregate_raw_day(
                monitor,
                AggregateBucketSize::Hour,
                day_start,
                day_end,
                &raw_results,
            );
            let day_aggregates = aggregate_raw_day(
                monitor,
                AggregateBucketSize::Day,
                day_start,
                day_end,
                &raw_results,
            );
            for aggregate in minute_aggregates
                .iter()
                .chain(hour_aggregates.iter())
                .chain(day_aggregates.iter())
            {
                aggregates::upsert_tx(&mut tx, aggregate).await?;
            }
        }

        let delete_until = day_end.min(raw_delete_cutoff);
        if delete_until > day_start {
            checks::delete_for_monitor_between_tx(&mut tx, &monitor.id, day_start, delete_until)
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

/// 将某一天的原始探测点按指定粒度聚合。
pub fn aggregate_raw_day(
    monitor: &Monitor,
    bucket_size: AggregateBucketSize,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
    results: &[CheckResult],
) -> Vec<CheckAggregate> {
    let mut grouped: BTreeMap<DateTime<Utc>, Vec<&CheckResult>> = BTreeMap::new();
    for result in results {
        let offset_seconds = result
            .checked_at
            .signed_duration_since(day_start)
            .num_seconds();
        if offset_seconds >= 0 {
            let bucket_seconds = bucket_size.seconds();
            // bucket_start 按 day_start 对齐，确保所有粒度都遵循配置时区的日历边界。
            let bucket_start =
                day_start + Duration::seconds((offset_seconds / bucket_seconds) * bucket_seconds);
            grouped.entry(bucket_start).or_default().push(result);
        }
    }

    let mut aggregates = Vec::new();
    let mut bucket_start = day_start;
    let step = Duration::seconds(bucket_size.seconds());
    while bucket_start < day_end {
        let bucket_end = (bucket_start + step).min(day_end);
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
        monitor_id: monitor.id.clone(),
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
            id: "m1".into(),
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
    fn raw_minute_unknown_count_uses_interval() {
        let monitor = monitor();
        let day_start = Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap();
        let day_end = day_start + Duration::minutes(1);
        let mut result = CheckResult::success("m1".into(), 42);
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
}
