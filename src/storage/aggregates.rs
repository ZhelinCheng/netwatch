//! 探测结果聚合 repository。

use chrono::{DateTime, Utc};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{
    domain::check::{AggregateBucketSize, CheckAggregate},
    error::AppError,
    storage::time::{from_timestamp_seconds, to_timestamp_seconds},
};

/// 查询指定时间范围内的聚合结果。
pub async fn list_for_monitor_between(
    pool: &SqlitePool,
    monitor_id: i64,
    bucket_size: AggregateBucketSize,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CheckAggregate>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, bucket_size, bucket_start, bucket_end,
               success_count, failed_count, unknown_count,
               latency_count, latency_sum_us, min_latency_us, max_latency_us,
               p95_latency_us, latency_buckets_json, created_at, updated_at
        FROM check_aggregates
        WHERE monitor_id = ? AND bucket_size = ? AND bucket_start >= ? AND bucket_start < ?
        ORDER BY bucket_start ASC
        "#,
    )
    .bind(monitor_id)
    .bind(bucket_size.as_str())
    .bind(to_timestamp_seconds(from))
    .bind(to_timestamp_seconds(to))
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(row_to_aggregate).collect()
}

/// 在事务内查询某天的聚合结果，用于判断是否已经压缩过。
pub async fn list_for_monitor_day_tx(
    tx: &mut Transaction<'_, Sqlite>,
    monitor_id: i64,
    bucket_size: AggregateBucketSize,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Vec<CheckAggregate>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, monitor_id, bucket_size, bucket_start, bucket_end,
               success_count, failed_count, unknown_count,
               latency_count, latency_sum_us, min_latency_us, max_latency_us,
               p95_latency_us, latency_buckets_json, created_at, updated_at
        FROM check_aggregates
        WHERE monitor_id = ? AND bucket_size = ? AND bucket_start >= ? AND bucket_start < ?
        ORDER BY bucket_start ASC
        "#,
    )
    .bind(monitor_id)
    .bind(bucket_size.as_str())
    .bind(to_timestamp_seconds(day_start))
    .bind(to_timestamp_seconds(day_end))
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter().map(row_to_aggregate).collect()
}

/// 以监控项、粒度和桶起点作为唯一键写入或更新聚合结果。
pub async fn upsert_tx(
    tx: &mut Transaction<'_, Sqlite>,
    aggregate: &CheckAggregate,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO check_aggregates (
            monitor_id, bucket_size, bucket_start, bucket_end,
            success_count, failed_count, unknown_count,
            latency_count, latency_sum_us, min_latency_us, max_latency_us,
            p95_latency_us, latency_buckets_json, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(monitor_id, bucket_size, bucket_start) DO UPDATE SET
            bucket_end = excluded.bucket_end,
            success_count = excluded.success_count,
            failed_count = excluded.failed_count,
            unknown_count = excluded.unknown_count,
            latency_count = excluded.latency_count,
            latency_sum_us = excluded.latency_sum_us,
            min_latency_us = excluded.min_latency_us,
            max_latency_us = excluded.max_latency_us,
            p95_latency_us = excluded.p95_latency_us,
            latency_buckets_json = excluded.latency_buckets_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(aggregate.monitor_id)
    .bind(aggregate.bucket_size.as_str())
    .bind(to_timestamp_seconds(aggregate.bucket_start))
    .bind(to_timestamp_seconds(aggregate.bucket_end))
    .bind(aggregate.success_count as i64)
    .bind(aggregate.failed_count as i64)
    .bind(aggregate.unknown_count as i64)
    .bind(aggregate.latency_count as i64)
    .bind(aggregate.latency_sum_us as i64)
    .bind(aggregate.min_latency_us.map(|value| value as i64))
    .bind(aggregate.max_latency_us.map(|value| value as i64))
    .bind(aggregate.p95_latency_us.map(|value| value as i64))
    .bind(serde_json::to_string(&aggregate.latency_buckets)?)
    .bind(to_timestamp_seconds(aggregate.created_at))
    .bind(to_timestamp_seconds(aggregate.updated_at))
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// 删除某个监控项的全部聚合结果。
pub async fn delete_for_monitor(pool: &SqlitePool, monitor_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM check_aggregates WHERE monitor_id = ?")
        .bind(monitor_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 删除指定保留边界之前的聚合结果。
pub async fn delete_older_than(
    pool: &SqlitePool,
    monitor_id: i64,
    bucket_size: AggregateBucketSize,
    cutoff: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        DELETE FROM check_aggregates
        WHERE monitor_id = ? AND bucket_size = ? AND bucket_start < ?
        "#,
    )
    .bind(monitor_id)
    .bind(bucket_size.as_str())
    .bind(to_timestamp_seconds(cutoff))
    .execute(pool)
    .await?;

    Ok(())
}

/// 将 SQLx row 转回领域模型。
fn row_to_aggregate(row: sqlx::sqlite::SqliteRow) -> Result<CheckAggregate, AppError> {
    let bucket_size: String = row.try_get("bucket_size")?;
    let bucket_start: i64 = row.try_get("bucket_start")?;
    let bucket_end: i64 = row.try_get("bucket_end")?;
    let created_at: i64 = row.try_get("created_at")?;
    let updated_at: i64 = row.try_get("updated_at")?;
    let latency_buckets_json: String = row.try_get("latency_buckets_json")?;

    Ok(CheckAggregate {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        bucket_size: AggregateBucketSize::from(bucket_size.as_str()),
        bucket_start: parse_time(bucket_start)?,
        bucket_end: parse_time(bucket_end)?,
        success_count: row.try_get::<i64, _>("success_count")? as u64,
        failed_count: row.try_get::<i64, _>("failed_count")? as u64,
        unknown_count: row.try_get::<i64, _>("unknown_count")? as u64,
        latency_count: row.try_get::<i64, _>("latency_count")? as u64,
        latency_sum_us: row.try_get::<i64, _>("latency_sum_us")? as u64,
        min_latency_us: row
            .try_get::<Option<i64>, _>("min_latency_us")?
            .map(|value| value as u64),
        max_latency_us: row
            .try_get::<Option<i64>, _>("max_latency_us")?
            .map(|value| value as u64),
        p95_latency_us: row
            .try_get::<Option<i64>, _>("p95_latency_us")?
            .map(|value| value as u64),
        latency_buckets: serde_json::from_str(&latency_buckets_json)?,
        created_at: parse_time(created_at)?,
        updated_at: parse_time(updated_at)?,
    })
}

/// 解析数据库中以 Unix 秒保存的 UTC 时间。
fn parse_time(value: i64) -> Result<DateTime<Utc>, AppError> {
    from_timestamp_seconds(value)
}
