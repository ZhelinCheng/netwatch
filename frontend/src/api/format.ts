import type { AlertKind, CheckSeriesPoint, CheckStatus, Monitor, MonitorKind } from './types'

export const kindLabel: Record<MonitorKind, string> = {
  http: 'HTTP',
  dns: 'DNS',
  tcp: 'TCP',
  ping: 'Ping',
}

export const statusLabel: Record<CheckStatus, string> = {
  success: '正常',
  failed: '故障',
  unknown: '未知',
}

export const alertLabel: Record<AlertKind, string> = {
  triggered: '已触发',
  recovered: '已恢复',
  certificate_expiring: '证书到期',
}

export function latencyMs(latencyUs?: number | null) {
  if (latencyUs == null) return '-'
  const ms = latencyUs / 1000
  return ms >= 100 ? `${Math.round(ms)}ms` : `${Number(ms.toFixed(1))}ms`
}

export function compactTime(timestamp?: number | null) {
  if (!timestamp) return '-'
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(new Date(timestamp * 1000))
}

export function dateTime(timestamp?: number | null) {
  if (!timestamp) return '-'
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(new Date(timestamp * 1000))
}

export function relativeTime(timestamp?: number | null) {
  if (!timestamp) return '-'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000 - timestamp))
  if (seconds < 60) return `${seconds} 秒前`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} 分钟前`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} 小时前`
  return `${Math.floor(hours / 24)} 天前`
}

export function availabilityFromLatest(monitors: Monitor[], status: CheckStatus | undefined) {
  if (status === 'success') return 100
  if (status === 'failed') return 0
  return monitors.length ? 0 : 0
}

export function seriesTimestamp(point: CheckSeriesPoint) {
  return point.kind === 'raw' ? point.checked_at : point.bucket_start
}

export function seriesLatencyMs(point: CheckSeriesPoint) {
  const value = point.kind === 'raw' ? point.latency_us : point.avg_latency_us
  return value == null ? null : Number((value / 1000).toFixed(2))
}

export function seriesStatus(point: CheckSeriesPoint): CheckStatus {
  if (point.kind === 'raw') return point.status
  if (point.failed_count > 0) return 'failed'
  if (point.success_count > 0) return 'success'
  return 'unknown'
}

export function clampPercent(value: number) {
  return Math.max(0, Math.min(100, value))
}
