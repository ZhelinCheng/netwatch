import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { compactTime, latencyMs, seriesStatus, seriesTimestamp } from '../api/format'
import type { CheckSeriesPoint } from '../api/types'
import styles from './StatusChart.module.scss'

interface StatusChartProps {
  points: CheckSeriesPoint[]
  height?: number
}

export function StatusChart({ points, height = 240 }: StatusChartProps) {
  const timestamps = points.map(seriesTimestamp).filter(Boolean)
  const spanSeconds = timestamps.length ? Math.max(...timestamps) - Math.min(...timestamps) : 0
  const data = points
    .map((point) => {
      const latency = point.kind === 'raw' ? usToMs(point.latency_us) : usToMs(point.avg_latency_us)
      const minLatency = point.kind === 'raw' ? latency : usToMs(point.min_latency_us)
      const maxLatency = point.kind === 'raw' ? latency : usToMs(point.max_latency_us)
      return {
        time: seriesTimestamp(point),
        label: chartTimeLabel(seriesTimestamp(point), spanSeconds),
        tooltipLabel: chartTooltipLabel(seriesTimestamp(point), spanSeconds),
        latency,
        minLatency,
        maxLatency,
        latencyRange: minLatency == null || maxLatency == null ? null : [minLatency, maxLatency],
        status: seriesStatus(point),
      }
    })
    .filter((point) => point.time)

  return (
    <div className={styles.chart}>
      <ResponsiveContainer width="100%" height={height}>
        <ComposedChart data={data} margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
          <CartesianGrid stroke="#edf1f6" strokeDasharray="3 3" />
          <XAxis
            dataKey="label"
            tickLine={false}
            axisLine={false}
            minTickGap={28}
            tick={{ fill: '#667085', fontSize: 12 }}
          />
          <YAxis
            tickLine={false}
            axisLine={false}
            width={42}
            tick={{ fill: '#667085', fontSize: 12 }}
          />
          <Tooltip
            content={<LatencyTooltip />}
            labelFormatter={(_, payload) => `检查时间 ${payload?.[0]?.payload?.tooltipLabel ?? '-'}`}
            contentStyle={{
              border: '1px solid #dde5ef',
              borderRadius: 8,
              boxShadow: '0 12px 32px rgba(15, 23, 42, 0.12)',
            }}
          />
          <Area
            type="linear"
            dataKey="latencyRange"
            fill="#b9d7ff"
            fillOpacity={0.42}
            stroke="none"
            connectNulls={false}
            isAnimationActive={false}
          />
          <Line
            type="linear"
            dataKey="latency"
            stroke="#176df2"
            strokeWidth={2}
            dot={false}
            connectNulls={false}
            activeDot={{ r: 4 }}
            isAnimationActive={false}
          />
        </ComposedChart>
      </ResponsiveContainer>
      <div className={styles.timeline} aria-label="状态时间轴">
        {data.slice(-90).map((point, index) => (
          <span key={`${point.time}-${index}`} className={styles[point.status]} title={point.label} />
        ))}
      </div>
    </div>
  )
}

interface ChartPayload {
  payload?: {
    tooltipLabel?: string
    latency?: number | null
    minLatency?: number | null
    maxLatency?: number | null
  }
}

interface TooltipContentProps {
  active?: boolean
  payload?: ChartPayload[]
}

function LatencyTooltip({ active, payload }: TooltipContentProps) {
  const point = payload?.find((item) => item.payload?.tooltipLabel)?.payload
  if (!active || !point) return null

  return (
    <div className={styles.tooltip}>
      <strong>检查时间 {point.tooltipLabel}</strong>
      <span>最高延迟：{msLabel(point.maxLatency)}</span>
      <span>平均延迟：{msLabel(point.latency)}</span>
      <span>最低延迟：{msLabel(point.minLatency)}</span>
    </div>
  )
}

function usToMs(value?: number | null) {
  return value == null ? null : Number((value / 1000).toFixed(2))
}

function msLabel(value?: number | null) {
  return value == null ? '-' : latencyMs(value * 1000)
}

function chartTimeLabel(timestamp: number, spanSeconds: number) {
  if (spanSeconds > 90 * 24 * 60 * 60) {
    return new Intl.DateTimeFormat('zh-CN', {
      year: '2-digit',
      month: '2-digit',
      day: '2-digit',
    }).format(new Date(timestamp * 1000))
  }
  if (spanSeconds > 24 * 60 * 60) {
    return new Intl.DateTimeFormat('zh-CN', {
      month: '2-digit',
      day: '2-digit',
    }).format(new Date(timestamp * 1000))
  }
  return compactTime(timestamp)
}

function chartTooltipLabel(timestamp: number, spanSeconds: number) {
  if (spanSeconds > 24 * 60 * 60) {
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
  return compactTime(timestamp)
}
