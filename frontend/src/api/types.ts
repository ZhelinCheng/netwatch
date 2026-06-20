export type MonitorKind = 'http' | 'dns' | 'tcp' | 'ping'
export type CheckStatus = 'success' | 'failed' | 'unknown'
export type AlertKind = 'triggered' | 'recovered' | 'certificate_expiring'
export type CompareOp = 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte'
export type TextOp = 'contains' | 'equals' | 'not_contains' | 'not_equals'

export type SuccessRule =
  | { type: 'http_status'; op: CompareOp; value: number }
  | { type: 'http_body'; op: TextOp; value: string }
  | { type: 'http_header'; key: string; op: TextOp; value: string }
  | { type: 'dns_answer'; op: TextOp; value: string }
  | { type: 'latency'; op: CompareOp; value_us: number }

export interface MonitorConfig {
  expected_status?: number | null
  expected_status_min?: number | null
  expected_status_max?: number | null
  keyword?: string | null
  dns_record?: string | null
  expected_value?: string | null
  success_rules?: SuccessRule[] | null
}

export interface Monitor {
  id: number
  name: string
  kind: MonitorKind
  target: string
  config: MonitorConfig
  interval_seconds: number
  timeout_seconds: number
  enabled: boolean
  created_at: number
  updated_at: number
}

export interface MonitorPayload {
  name: string
  kind: MonitorKind
  target: string
  config: MonitorConfig
  interval_seconds: number
  timeout_seconds: number
  enabled: boolean
}

export interface CheckResult {
  id: number | null
  monitor_id: number
  status: CheckStatus
  latency_us: number | null
  checked_at: number
}

export interface AggregatePoint {
  monitor_id: number
  bucket_size: 'minute' | 'hour' | 'day'
  bucket_start: number
  bucket_end: number
  success_count: number
  failed_count: number
  unknown_count: number
  availability: number
  avg_latency_us: number | null
  p95_latency_us: number | null
  min_latency_us: number | null
  max_latency_us: number | null
}

export type CheckSeriesPoint =
  | ({ kind: 'raw' } & CheckResult)
  | ({ kind: 'aggregate' } & AggregatePoint)

export interface LatencyMetrics {
  total: number
  success: number
  failed: number
  unknown: number
  availability: number
  average_latency_us: number | null
  p95_latency_us: number | null
}

export interface ChecksResponse {
  resolution: string
  metrics: LatencyMetrics
  results: CheckSeriesPoint[]
}

export interface AlertEvent {
  id: number | null
  monitor_id: number
  kind: AlertKind
  message: string
  delivered: boolean
  created_at: number
}

export interface Dashboard {
  monitors: Monitor[]
  latest: Record<string, CheckResult>
  alerts: AlertEvent[]
  total: number
  success: number
  failed: number
  unknown: number
}
