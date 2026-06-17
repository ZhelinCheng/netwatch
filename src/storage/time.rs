//! 数据库存储使用的时间转换工具。

use chrono::{DateTime, Utc};

use crate::error::AppError;

/// 将 UTC 时间转换为 Unix 秒级时间戳。
pub fn to_timestamp_seconds(value: DateTime<Utc>) -> i64 {
    value.timestamp()
}

/// 将 Unix 秒级时间戳转换为 UTC 时间。
pub fn from_timestamp_seconds(value: i64) -> Result<DateTime<Utc>, AppError> {
    DateTime::<Utc>::from_timestamp(value, 0)
        .ok_or_else(|| AppError::BadRequest(format!("invalid unix timestamp seconds: {value}")))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn timestamp_seconds_round_trips_utc_time() {
        let time = Utc.with_ymd_and_hms(2026, 6, 17, 8, 9, 10).unwrap();
        let timestamp = to_timestamp_seconds(time);

        assert_eq!(from_timestamp_seconds(timestamp).unwrap(), time);
    }
}
