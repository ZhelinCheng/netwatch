import { AlertCircle, CheckCircle2, HelpCircle } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Badge } from '../components/Badge'
import { EmptyState } from '../components/EmptyState'
import { StatusChart } from '../components/StatusChart'
import { alertLabel, compactTime, intervalLabel, kindLabel, latencyMs, statusLabel } from '../api/format'
import { netwatchApi } from '../api/netwatch'
import type { Monitor } from '../api/types'
import styles from './pages.module.scss'

function badgeTone(kind: Monitor['kind']) {
  if (kind === 'http') return 'blue'
  if (kind === 'dns') return 'cyan'
  if (kind === 'tcp') return 'purple'
  return 'orange'
}

export function DashboardPage() {
  const dashboard = useQuery({
    queryKey: ['dashboard'],
    queryFn: netwatchApi.dashboard,
    refetchInterval: 30_000,
  })

  const chartMonitor = dashboard.data?.monitors[0]
  const checks = useQuery({
    queryKey: ['checks', chartMonitor?.id, 'dashboard'],
    enabled: Boolean(chartMonitor),
    queryFn: () => {
      const to = Math.floor(Date.now() / 1000)
      return netwatchApi.checks(chartMonitor!.id, { from: to - 60 * 60, to })
    },
    refetchInterval: 30_000,
  })

  const data = dashboard.data
  const monitors = data?.monitors ?? []
  const enabledCount = monitors.filter((monitor) => monitor.enabled).length
  const latest = data?.latest ?? {}
  const p95Values = Object.values(latest).flatMap((result) =>
    result.latency_us == null ? [] : [result.latency_us],
  )
  const p95 = p95Values.length ? Math.max(...p95Values) : null

  return (
    <div className={styles.page}>
      <section className={styles.gridStats}>
	        <div className={styles.statCard}>
	          <span>全部监控</span>
	          <strong>{data?.total ?? 0}</strong>
	          <small>{enabledCount} / {data?.total ?? 0} 启用/全部</small>
	        </div>
        <div className={`${styles.statCard} ${styles.statGreen}`}>
          <span>正常</span>
          <strong>{data?.success ?? 0}</strong>
          <small>{data?.total ? `${((data.success / data.total) * 100).toFixed(1)}%` : '0%'}</small>
        </div>
        <div className={`${styles.statCard} ${styles.statRed}`}>
          <span>故障</span>
          <strong>{data?.failed ?? 0}</strong>
          <small>{data?.total ? `${((data.failed / data.total) * 100).toFixed(1)}%` : '0%'}</small>
        </div>
        <div className={styles.statCard}>
          <span>未知</span>
          <strong>{data?.unknown ?? 0}</strong>
        </div>
        <div className={`${styles.statCard} ${styles.statBlue}`}>
          <span>P95 延迟</span>
          <strong>{latencyMs(p95)}</strong>
          <small>基于最近一次检查</small>
        </div>
      </section>

      <section className={styles.dashboardGrid}>
        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <h2>延迟与可用性</h2>
            <span>{chartMonitor?.name ?? '暂无监控项'}</span>
          </div>
          <div className={styles.chartBody}>
            {checks.data?.results?.length ? (
              <StatusChart points={checks.data.results} />
            ) : (
              <EmptyState title="暂无趋势数据" description="等待调度器写入检查结果后将显示延迟曲线。" />
            )}
          </div>
        </div>

        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <h2>最近告警</h2>
            <Link className={styles.ghostButton} to="/alerts">
              查看全部
            </Link>
          </div>
          <div className={styles.alertList}>
            {data?.alerts.length ? (
              data.alerts.map((alert) => (
                <div className={styles.alertItem} key={alert.id ?? `${alert.monitor_id}-${alert.created_at}`}>
                  {alert.kind === 'recovered' ? (
                    <CheckCircle2 className={styles.ok} size={18} />
                  ) : (
                    <AlertCircle className={styles.bad} size={18} />
                  )}
                  <div>
                    <strong>{alert.message}</strong>
                    <p>{alertLabel[alert.kind]}</p>
                  </div>
                  <time>{compactTime(alert.created_at)}</time>
                </div>
              ))
            ) : (
              <EmptyState title="暂无告警" />
            )}
          </div>
        </div>
      </section>

      <section className={styles.card}>
        <div className={styles.cardHeader}>
          <h2>监控项概览</h2>
          <Link className={styles.button} to="/monitors">
            管理全部
          </Link>
        </div>
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>名称</th>
                <th>类型</th>
                <th>目标</th>
                <th>状态</th>
                <th>延迟</th>
                <th>最近可用</th>
                <th>最近检查</th>
                <th>间隔</th>
              </tr>
            </thead>
            <tbody>
              {monitors.map((monitor) => {
                const result = latest[String(monitor.id)]
                const status = result?.status ?? 'unknown'
                return (
                  <tr key={monitor.id}>
                    <td>
                      <Link className={styles.nameCell} to={`/monitors/${monitor.id}`}>
                        <strong>{monitor.name}</strong>
                      </Link>
                    </td>
                    <td>
                      <Badge tone={badgeTone(monitor.kind)}>{kindLabel[monitor.kind]}</Badge>
                    </td>
                    <td>{monitor.target}</td>
                    <td>
                      <Badge tone={status === 'success' ? 'green' : status === 'failed' ? 'red' : 'gray'}>
                        {statusLabel[status]}
                      </Badge>
                    </td>
                    <td>{latencyMs(result?.latency_us)}</td>
                    <td>
                      <span className={styles.bar}>
                        <span style={{ width: status === 'success' ? '100%' : '0%' }} />
                      </span>
                    </td>
                    <td>{compactTime(result?.checked_at)}</td>
                    <td>{intervalLabel(monitor.interval_seconds)}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
        {!monitors.length && !dashboard.isLoading ? (
          <EmptyState title="暂无监控项" description="创建第一个监控项后，这里会显示实时概览。" />
        ) : null}
      </section>

      {dashboard.isError ? (
        <div className={styles.error}>
          <HelpCircle size={16} /> Dashboard 数据加载失败，请确认后端服务已启动。
        </div>
      ) : null}
      {checks.isFetching ? <span aria-label="正在刷新趋势" hidden /> : null}
    </div>
  )
}
