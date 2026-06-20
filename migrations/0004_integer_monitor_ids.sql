PRAGMA foreign_keys = OFF;

DROP INDEX IF EXISTS idx_check_results_monitor_checked_at;
DROP INDEX IF EXISTS idx_alert_events_created_at;
DROP INDEX IF EXISTS idx_check_aggregates_monitor_bucket_start;

DROP TABLE IF EXISTS check_aggregates;
DROP TABLE IF EXISTS alert_events;
DROP TABLE IF EXISTS check_results;
DROP TABLE IF EXISTS monitors;

CREATE TABLE monitors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    config_json TEXT NOT NULL,
    interval_seconds INTEGER NOT NULL,
    timeout_seconds INTEGER NOT NULL,
    enabled INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE check_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    monitor_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    latency_us INTEGER,
    checked_at INTEGER NOT NULL,
    FOREIGN KEY (monitor_id) REFERENCES monitors(id) ON DELETE CASCADE
);

CREATE TABLE alert_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    monitor_id INTEGER NOT NULL,
    kind TEXT NOT NULL,
    message TEXT NOT NULL,
    delivered INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (monitor_id) REFERENCES monitors(id) ON DELETE CASCADE
);

CREATE TABLE check_aggregates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    monitor_id INTEGER NOT NULL,
    bucket_size TEXT NOT NULL,
    bucket_start INTEGER NOT NULL,
    bucket_end INTEGER NOT NULL,
    success_count INTEGER NOT NULL,
    failed_count INTEGER NOT NULL,
    unknown_count INTEGER NOT NULL,
    latency_count INTEGER NOT NULL,
    latency_sum_us INTEGER NOT NULL,
    min_latency_us INTEGER,
    max_latency_us INTEGER,
    p95_latency_us INTEGER,
    latency_buckets_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(monitor_id, bucket_size, bucket_start),
    FOREIGN KEY (monitor_id) REFERENCES monitors(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_check_results_monitor_checked_at
    ON check_results(monitor_id, checked_at DESC);

CREATE INDEX IF NOT EXISTS idx_alert_events_created_at
    ON alert_events(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_check_aggregates_monitor_bucket_start
    ON check_aggregates(monitor_id, bucket_size, bucket_start);

PRAGMA foreign_keys = ON;
