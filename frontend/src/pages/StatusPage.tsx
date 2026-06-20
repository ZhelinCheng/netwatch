import { useQuery } from '@tanstack/react-query'
import { Badge } from '../components/Badge'
import { EmptyState } from '../components/EmptyState'
import { compactTime, kindLabel, latencyMs, statusLabel } from '../api/format'
import { netwatchApi } from '../api/netwatch'
import styles from './pages.module.scss'

export function StatusPage() {
  const dashboard = useQuery({
    queryKey: ['dashboard'],
    queryFn: netwatchApi.dashboard,
    refetchInterval: 30_000,
  })

  const monitors = dashboard.data?.monitors ?? []

  return (
    <div className={styles.page}>
      <div className={styles.pageHeader}>
        <div>
          <h1>状态页</h1>
          <p>面向公开展示的服务运行状态概览。</p>
        </div>
      </div>

      <section className={styles.gridStats}>
        <div className={styles.statCard}>
          <span>服务总数</span>
          <strong>{dashboard.data?.total ?? 0}</strong>
        </div>
        <div className={`${styles.statCard} ${styles.statGreen}`}>
          <span>正常服务</span>
          <strong>{dashboard.data?.success ?? 0}</strong>
        </div>
        <div className={`${styles.statCard} ${styles.statRed}`}>
          <span>故障服务</span>
          <strong>{dashboard.data?.failed ?? 0}</strong>
        </div>
        <div className={styles.statCard}>
          <span>未知服务</span>
          <strong>{dashboard.data?.unknown ?? 0}</strong>
        </div>
      </section>

      <section className={styles.card}>
        <div className={styles.cardHeader}>
          <h2>服务状态</h2>
        </div>
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>服务</th>
                <th>类型</th>
                <th>状态</th>
                <th>延迟</th>
                <th>最近检查</th>
              </tr>
            </thead>
            <tbody>
              {monitors.map((monitor) => {
                const latest = dashboard.data?.latest[String(monitor.id)]
                const status = latest?.status ?? 'unknown'
                return (
                  <tr key={monitor.id}>
                    <td>
                      <div className={styles.nameCell}>
                        <strong>{monitor.name}</strong>
                        <span>{monitor.target}</span>
                      </div>
                    </td>
                    <td>{kindLabel[monitor.kind]}</td>
                    <td>
                      <Badge tone={status === 'success' ? 'green' : status === 'failed' ? 'red' : 'gray'}>
                        {statusLabel[status]}
                      </Badge>
                    </td>
                    <td>{latencyMs(latest?.latency_us)}</td>
                    <td>{compactTime(latest?.checked_at)}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
        {!monitors.length && !dashboard.isLoading ? <EmptyState title="暂无服务状态" /> : null}
      </section>
    </div>
  )
}
