import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { compactTime, latencyMs, seriesLatencyMs, seriesStatus, seriesTimestamp } from '../api/format'
import type { CheckSeriesPoint } from '../api/types'
import styles from './StatusChart.module.scss'

interface StatusChartProps {
  points: CheckSeriesPoint[]
  height?: number
}

export function StatusChart({ points, height = 240 }: StatusChartProps) {
  const data = points
    .map((point) => ({
      time: seriesTimestamp(point),
      label: compactTime(seriesTimestamp(point)),
      latency: seriesLatencyMs(point),
      status: seriesStatus(point),
    }))
    .filter((point) => point.time)

  return (
    <div className={styles.chart}>
      <ResponsiveContainer width="100%" height={height}>
        <LineChart data={data} margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
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
            formatter={(value) => [latencyMs(Number(value) * 1000), '延迟']}
            labelFormatter={(label) => `检查时间 ${label}`}
            contentStyle={{
              border: '1px solid #dde5ef',
              borderRadius: 8,
              boxShadow: '0 12px 32px rgba(15, 23, 42, 0.12)',
            }}
          />
          <Line
            type="monotone"
            dataKey="latency"
            stroke="#176df2"
            strokeWidth={2}
            dot={false}
            connectNulls={false}
            activeDot={{ r: 4 }}
          />
        </LineChart>
      </ResponsiveContainer>
      <div className={styles.timeline} aria-label="状态时间轴">
        {data.slice(-90).map((point, index) => (
          <span key={`${point.time}-${index}`} className={styles[point.status]} title={point.label} />
        ))}
      </div>
    </div>
  )
}
