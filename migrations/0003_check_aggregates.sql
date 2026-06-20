CREATE TABLE IF NOT EXISTS check_aggregates (
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

CREATE INDEX IF NOT EXISTS idx_check_aggregates_monitor_bucket_start
    ON check_aggregates(monitor_id, bucket_size, bucket_start);
