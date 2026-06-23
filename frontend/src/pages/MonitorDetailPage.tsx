import { Edit3, Globe2, Pause, Play, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Badge } from '../components/Badge'
import { EmptyState } from '../components/EmptyState'
import { StatusChart } from '../components/StatusChart'
import { compactTime, dateTime, intervalLabel, kindLabel, latencyMs, statusLabel } from '../api/format'
import { netwatchApi } from '../api/netwatch'
import styles from './pages.module.scss'

const chartRanges = [
  { key: '24h', label: '24 小时', seconds: 24 * 60 * 60 },
  { key: '7d', label: '7 日', seconds: 7 * 24 * 60 * 60 },
  { key: '30d', label: '30 日', seconds: 30 * 24 * 60 * 60 },
  { key: '365d', label: '365 日', seconds: 365 * 24 * 60 * 60 },
] as const

type ChartRangeKey = (typeof chartRanges)[number]['key']

export function MonitorDetailPage() {
  const { id } = useParams()
  const monitorId = Number(id)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [chartRange, setChartRange] = useState<ChartRangeKey>('24h')
  const selectedChartRange = chartRanges.find((range) => range.key === chartRange) ?? chartRanges[0]

  const monitor = useQuery({
    queryKey: ['monitor', monitorId],
    enabled: Number.isFinite(monitorId),
    queryFn: () => netwatchApi.monitor(monitorId),
  })

  const checks = useQuery({
    queryKey: ['checks', monitorId, 'detail', chartRange],
    enabled: Number.isFinite(monitorId),
    queryFn: () => {
      const to = Math.floor(Date.now() / 1000)
      return netwatchApi.checks(monitorId, { from: to - selectedChartRange.seconds, to })
    },
    refetchInterval: 30_000,
  })

  const latestChecks = useQuery({
    queryKey: ['checks', monitorId, 'latest'],
    enabled: Number.isFinite(monitorId),
    queryFn: () => netwatchApi.checks(monitorId, { limit: 20 }),
    refetchInterval: 30_000,
  })

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ['monitor', monitorId] })
    queryClient.invalidateQueries({ queryKey: ['dashboard'] })
  }

  const toggleMutation = useMutation({
    mutationFn: () =>
      monitor.data?.enabled ? netwatchApi.pauseMonitor(monitorId) : netwatchApi.resumeMonitor(monitorId),
    onSuccess: invalidate,
  })

  const deleteMutation = useMutation({
    mutationFn: () => netwatchApi.deleteMonitor(monitorId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
      navigate('/monitors')
    },
  })

  const latest = latestChecks.data?.results.find((point) => point.kind === 'raw')
  const status = latest?.kind === 'raw' ? latest.status : 'unknown'

  if (monitor.isLoading) {
    return <EmptyState title="正在加载监控项" />
  }

  if (!monitor.data) {
    return <EmptyState title="监控项不存在" description="请返回列表重新选择。" />
  }

  return (
    <div className={styles.page}>
      <section className={styles.card}>
        <div className={styles.detailHero}>
          <div className={styles.detailTitle}>
            <Globe2 size={28} />
            <div>
              <h1>{monitor.data.name}</h1>
              <p>
                类型：{kindLabel[monitor.data.kind]} 目标：{monitor.data.target} ID：{monitor.data.id}
              </p>
            </div>
            <Badge tone={status === 'success' ? 'green' : status === 'failed' ? 'red' : 'gray'}>
              {statusLabel[status]}
            </Badge>
          </div>
          <div className={styles.filterGroup}>
            <button className={styles.ghostButton} type="button" onClick={() => toggleMutation.mutate()}>
              {monitor.data.enabled ? <Pause size={16} /> : <Play size={16} />}
              {monitor.data.enabled ? '暂停' : '恢复'}
            </button>
            <Link className={styles.button} to={`/monitors/${monitor.data.id}/edit`}>
              <Edit3 size={16} /> 编辑
            </Link>
            <button
              className={styles.dangerButton}
              type="button"
              onClick={() => {
                if (window.confirm(`确认删除监控项「${monitor.data.name}」？`)) {
                  deleteMutation.mutate()
                }
              }}
            >
              <Trash2 size={16} /> 删除
            </button>
          </div>
        </div>
      </section>

      <section className={styles.detailStats}>
        <div className={styles.statCard}>
          <span>当前状态</span>
          <strong className={status === 'success' ? styles.ok : status === 'failed' ? styles.bad : ''}>
            {statusLabel[status]}
          </strong>
          <small>自 {compactTime(latest?.kind === 'raw' ? latest.checked_at : null)} 起</small>
        </div>
        <div className={`${styles.statCard} ${styles.statBlue}`}>
          <span>最近延迟</span>
          <strong>{latencyMs(latest?.kind === 'raw' ? latest.latency_us : null)}</strong>
        </div>
        <div className={styles.statCard}>
          <span>{selectedChartRange.label} 可用率</span>
          <strong>{checks.data?.metrics.availability.toFixed(2) ?? '0.00'}%</strong>
        </div>
        <div className={styles.statCard}>
          <span>最近检查</span>
          <strong>{compactTime(latest?.kind === 'raw' ? latest.checked_at : null)}</strong>
          <small>{latest?.kind === 'raw' ? '1 分钟内' : '-'}</small>
        </div>
        <div className={styles.statCard}>
          <span>检查间隔</span>
          <strong>{intervalLabel(monitor.data.interval_seconds)}</strong>
          <small>超时：{monitor.data.timeout_seconds} 秒</small>
        </div>
      </section>

      <section className={styles.twoColumn}>
        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <h2>延迟与可用性</h2>
            <div className={styles.rangeTabs} aria-label="选择趋势时间范围">
              {chartRanges.map((range) => (
                <button
                  key={range.key}
                  type="button"
                  className={range.key === chartRange ? styles.rangeTabActive : ''}
                  onClick={() => setChartRange(range.key)}
                >
                  {range.label}
                </button>
              ))}
            </div>
          </div>
          <div className={styles.chartBody}>
            {checks.data?.results.length ? (
              <StatusChart points={checks.data.results} height={330} />
            ) : (
              <EmptyState title="暂无检查序列" />
            )}
          </div>
        </div>

        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <h2>配置摘要</h2>
          </div>
          <dl className={styles.summaryList}>
            <div>
              <dt>类型</dt>
              <dd>{kindLabel[monitor.data.kind]}</dd>
            </div>
            <div>
              <dt>目标</dt>
              <dd>{monitor.data.target}</dd>
            </div>
            {monitor.data.kind === 'http' ? (
              <>
                <div>
                  <dt>期望状态码</dt>
                  <dd>
                    {monitor.data.config.expected_status_min ?? 200} -{' '}
                    {monitor.data.config.expected_status_max ?? 399}
                  </dd>
                </div>
                <div>
                  <dt>响应关键词/正则</dt>
                  <dd>{monitor.data.config.keyword || '-'}</dd>
                </div>
                <div>
                  <dt>响应头匹配</dt>
                  <dd>{monitor.data.config.expected_headers?.length ?? 0} 条</dd>
                </div>
              </>
            ) : null}
            {monitor.data.kind === 'dns' ? (
              <>
                <div>
                  <dt>DNS 记录类型</dt>
                  <dd>{monitor.data.config.dns_record ?? 'A'}</dd>
                </div>
                <div>
                  <dt>期望解析值</dt>
                  <dd>{monitor.data.config.expected_value || '-'}</dd>
                </div>
              </>
            ) : null}
            <div>
              <dt>启用状态</dt>
              <dd>{monitor.data.enabled ? '启用' : '暂停'}</dd>
            </div>
            <div>
              <dt>创建时间</dt>
              <dd>{dateTime(monitor.data.created_at)}</dd>
            </div>
          </dl>
        </div>
      </section>

      <section className={styles.card}>
        <div className={styles.cardHeader}>
          <h2>最近检查结果</h2>
          <button className={styles.ghostButton} type="button" onClick={() => latestChecks.refetch()}>
            查看全部
          </button>
        </div>
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>检查时间</th>
                <th>状态</th>
                <th>延迟</th>
                <th>错误信息</th>
              </tr>
            </thead>
            <tbody>
              {latestChecks.data?.results
                .filter((point) => point.kind === 'raw')
                .map((point) => (
                  <tr key={`${point.checked_at}-${point.id}`}>
                    <td>{dateTime(point.checked_at)}</td>
                    <td>
                      <Badge tone={point.status === 'success' ? 'green' : point.status === 'failed' ? 'red' : 'gray'}>
                        {statusLabel[point.status]}
                      </Badge>
                    </td>
                    <td>{latencyMs(point.latency_us)}</td>
                    <td>{point.status === 'failed' ? '探测失败或超时' : '-'}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  )
}
