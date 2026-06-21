import { AlertCircle, CheckCircle2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Badge } from '../components/Badge'
import { EmptyState } from '../components/EmptyState'
import { Pagination } from '../components/Pagination'
import { alertLabel, dateTime, kindLabel, relativeTime } from '../api/format'
import { netwatchApi } from '../api/netwatch'
import type { AlertKind } from '../api/types'
import styles from './pages.module.scss'

export function AlertsPage() {
  const [kind, setKind] = useState<'all' | AlertKind>('all')
  const [monitorId, setMonitorId] = useState('all')
  const [searchInput, setSearchInput] = useState('')
  const [keyword, setKeyword] = useState('')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(10)

  const alerts = useQuery({
    queryKey: ['alerts'],
    queryFn: () => netwatchApi.alerts(500),
    refetchInterval: 30_000,
  })
  const dashboard = useQuery({
    queryKey: ['dashboard'],
    queryFn: netwatchApi.dashboard,
    refetchInterval: 30_000,
  })

  const monitorMap = useMemo(
    () => new Map((dashboard.data?.monitors ?? []).map((monitor) => [monitor.id, monitor])),
    [dashboard.data?.monitors],
  )
  const rows = useMemo(() => {
    const normalized = keyword.trim().toLowerCase()
    return (alerts.data ?? []).filter((alert) => {
      const monitor = monitorMap.get(alert.monitor_id)
      const matchesKind = kind === 'all' || alert.kind === kind
      const matchesMonitor = monitorId === 'all' || alert.monitor_id === Number(monitorId)
      const matchesKeyword =
        !normalized ||
        alert.message.toLowerCase().includes(normalized) ||
        monitor?.name.toLowerCase().includes(normalized) ||
        monitor?.target.toLowerCase().includes(normalized)
      return matchesKind && matchesMonitor && matchesKeyword
    })
  }, [alerts.data, kind, keyword, monitorId, monitorMap])

  const triggered = rows.filter((alert) => alert.kind === 'triggered').length
  const recovered = rows.filter((alert) => alert.kind === 'recovered').length
  const unknown = rows.length - triggered - recovered
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize))
  const pagedRows = rows.slice((page - 1) * pageSize, page * pageSize)

  function submitSearch() {
    setKeyword(searchInput.trim())
    setPage(1)
  }

  return (
    <div className={styles.page}>
      <div className={styles.pageHeader}>
        <div>
          <h1>告警</h1>
          <p>查看触发、恢复和通知投递记录。</p>
        </div>
      </div>

      <section className={styles.card}>
        <div className={styles.cardHeader}>
          <div className={styles.filterGroup}>
            <select className={styles.select} value={kind} onChange={(event) => setKind(event.target.value as typeof kind)}>
              <option value="all">事件类型：全部</option>
              <option value="triggered">已触发</option>
              <option value="recovered">已恢复</option>
              <option value="certificate_expiring">证书到期</option>
            </select>
            <select className={styles.select} value={monitorId} onChange={(event) => setMonitorId(event.target.value)}>
              <option value="all">监控项：全部</option>
              {(dashboard.data?.monitors ?? []).map((monitor) => (
                <option key={monitor.id} value={monitor.id}>
                  {monitor.name}
                </option>
              ))}
            </select>
            <input
              className={styles.search}
              placeholder="搜索监控项、目标或错误信息..."
              value={searchInput}
              onChange={(event) => setSearchInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') submitSearch()
              }}
            />
            <button type="button" className={styles.ghostButton} onClick={submitSearch}>
              搜索
            </button>
          </div>
          <button className={styles.ghostButton} type="button" onClick={() => alerts.refetch()}>
            刷新
          </button>
        </div>
      </section>

      <section className={styles.gridStats}>
        <div className={styles.statCard}>
          <span>全部事件</span>
          <strong>{rows.length}</strong>
        </div>
        <div className={`${styles.statCard} ${styles.statRed}`}>
          <span>已触发</span>
          <strong>{triggered}</strong>
        </div>
        <div className={`${styles.statCard} ${styles.statGreen}`}>
          <span>已恢复</span>
          <strong>{recovered}</strong>
        </div>
        <div className={styles.statCard}>
          <span>其他</span>
          <strong>{unknown}</strong>
        </div>
      </section>

      <section className={styles.card}>
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>事件</th>
                <th>监控项</th>
                <th>目标</th>
                <th>事件时间</th>
                <th>原因 / 错误信息</th>
                <th>当前状态</th>
              </tr>
            </thead>
            <tbody>
              {pagedRows.map((alert) => {
                const monitor = monitorMap.get(alert.monitor_id)
                const currentStatus = dashboard.data?.latest[String(alert.monitor_id)]?.status ?? 'unknown'
                return (
                  <tr key={alert.id ?? `${alert.monitor_id}-${alert.created_at}`}>
                    <td>
                      <div className={styles.nameCell}>
                        <strong>
                          {alert.kind === 'recovered' ? (
                            <CheckCircle2 className={styles.ok} size={14} />
                          ) : (
                            <AlertCircle className={styles.bad} size={14} />
                          )}{' '}
                          {alertLabel[alert.kind]}
                        </strong>
                        <span>{alert.delivered ? '已投递' : '未投递'}</span>
                      </div>
                    </td>
                    <td>
                      <div className={styles.nameCell}>
                        <strong>{monitor?.name ?? `#${alert.monitor_id}`}</strong>
                        <span>{monitor ? kindLabel[monitor.kind] : '-'}</span>
                      </div>
                    </td>
                    <td>{monitor?.target ?? '-'}</td>
                    <td>
                      <div className={styles.nameCell}>
                        <strong>{dateTime(alert.created_at)}</strong>
                        <span>{relativeTime(alert.created_at)}</span>
                      </div>
                    </td>
                    <td>{alert.message}</td>
                    <td>
                      <Badge tone={currentStatus === 'success' ? 'green' : currentStatus === 'failed' ? 'red' : 'gray'}>
                        {currentStatus === 'success' ? '正常' : currentStatus === 'failed' ? '故障' : '未知'}
                      </Badge>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
        {!rows.length && !alerts.isLoading ? <EmptyState title="暂无告警事件" /> : null}
        <Pagination
          page={page}
          pageCount={pageCount}
          total={rows.length}
          pageSize={pageSize}
          onPageChange={setPage}
          onPageSizeChange={(size) => {
            setPageSize(size)
            setPage(1)
          }}
        />
      </section>
    </div>
  )
}
