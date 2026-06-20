PRAGMA foreign_keys = OFF;

DROP INDEX IF EXISTS idx_check_results_monitor_checked_at;

ALTER TABLE check_results RENAME TO check_results_old;

CREATE TABLE check_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    monitor_id INTEGER NOT NULL,
    status TEXT NOT NULL,
    latency_us INTEGER,
    checked_at INTEGER NOT NULL,
    FOREIGN KEY (monitor_id) REFERENCES monitors(id) ON DELETE CASCADE
);

INSERT INTO check_results (id, monitor_id, status, latency_us, checked_at)
SELECT
    id,
    monitor_id,
    CASE
        WHEN status = 'up' THEN 'success'
        WHEN status = 'success' THEN 'success'
        ELSE 'failed'
    END,
    latency_us,
    checked_at
FROM check_results_old;

DROP TABLE check_results_old;

CREATE INDEX IF NOT EXISTS idx_check_results_monitor_checked_at
    ON check_results(monitor_id, checked_at DESC);

PRAGMA foreign_keys = ON;
