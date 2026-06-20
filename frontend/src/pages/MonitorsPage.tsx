import { Edit3, Pause, Play, Plus, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Badge } from '../components/Badge'
import { EmptyState } from '../components/EmptyState'
import { Pagination } from '../components/Pagination'
import { Switch } from '../components/Switch'
import { compactTime, kindLabel, latencyMs, statusLabel } from '../api/format'
import { netwatchApi } from '../api/netwatch'
import type { Monitor, MonitorKind } from '../api/types'
import styles from './pages.module.scss'

function kindTone(kind: MonitorKind) {
  return kind === 'http' ? 'blue' : kind === 'dns' ? 'cyan' : kind === 'tcp' ? 'purple' : 'orange'
}

export function MonitorsPage() {
  const queryClient = useQueryClient()
  const [keyword, setKeyword] = useState('')
  const [kind, setKind] = useState<'all' | MonitorKind>('all')
  const [status, setStatus] = useState<'all' | 'success' | 'failed' | 'unknown'>('all')
  const [enabled, setEnabled] = useState<'all' | 'enabled' | 'disabled'>('all')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(10)

  const dashboard = useQuery({
    queryKey: ['dashboard'],
    queryFn: netwatchApi.dashboard,
    refetchInterval: 30_000,
  })

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['dashboard'] })

  const toggleMutation = useMutation({
    mutationFn: (monitor: Monitor) =>
      monitor.enabled ? netwatchApi.pauseMonitor(monitor.id) : netwatchApi.resumeMonitor(monitor.id),
    onSuccess: invalidate,
  })

  const deleteMutation = useMutation({
    mutationFn: netwatchApi.deleteMonitor,
    onSuccess: invalidate,
  })

  const rows = useMemo(() => {
    const normalized = keyword.trim().toLowerCase()
    return (dashboard.data?.monitors ?? []).filter((monitor) => {
      const currentStatus = dashboard.data?.latest[String(monitor.id)]?.status ?? 'unknown'
      const matchesKeyword =
        !normalized ||
        monitor.name.toLowerCase().includes(normalized) ||
        monitor.target.toLowerCase().includes(normalized)
      const matchesKind = kind === 'all' || monitor.kind === kind
      const matchesStatus = status === 'all' || currentStatus === status
      const matchesEnabled =
        enabled === 'all' ||
        (enabled === 'enabled' && monitor.enabled) ||
        (enabled === 'disabled' && !monitor.enabled)
      return matchesKeyword && matchesKind && matchesStatus && matchesEnabled
    })
  }, [dashboard.data, enabled, keyword, kind, status])

  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize))
  const pagedRows = rows.slice((page - 1) * pageSize, page * pageSize)

  return (
    <div className={styles.page}>
      <div className={styles.pageHeader}>
        <div>
          <h1>监控项</h1>
          <p>管理 HTTP、DNS、TCP 和 Ping 监控项。</p>
        </div>
        <Link className={styles.button} to="/monitors/new">
          <Plus size={16} /> 添加监控项
        </Link>
      </div>

      <section className={styles.card}>
        <div className={styles.cardHeader}>
          <div className={styles.filterGroup}>
            <input
              className={styles.search}
              value={keyword}
              placeholder="搜索名称、目标..."
              onChange={(event) => {
                setKeyword(event.target.value)
                setPage(1)
              }}
            />
            <select className={styles.select} value={kind} onChange={(event) => setKind(event.target.value as typeof kind)}>
              <option value="all">类型：全部</option>
              <option value="http">HTTP</option>
              <option value="dns">DNS</option>
              <option value="tcp">TCP</option>
              <option value="ping">Ping</option>
            </select>
            <select className={styles.select} value={status} onChange={(event) => setStatus(event.target.value as typeof status)}>
              <option value="all">状态：全部</option>
              <option value="success">正常</option>
              <option value="failed">故障</option>
              <option value="unknown">未知</option>
            </select>
            <select className={styles.select} value={enabled} onChange={(event) => setEnabled(event.target.value as typeof enabled)}>
              <option value="all">启用状态：全部</option>
              <option value="enabled">已启用</option>
              <option value="disabled">已暂停</option>
            </select>
          </div>
          <button type="button" className={styles.ghostButton} onClick={() => dashboard.refetch()}>
            重置/刷新
          </button>
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
                <th>可用率 (24h)</th>
                <th>最近检查</th>
                <th>间隔</th>
                <th>启用</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {pagedRows.map((monitor) => {
                const result = dashboard.data?.latest[String(monitor.id)]
                const currentStatus = result?.status ?? 'unknown'
                const percent = currentStatus === 'success' ? 100 : 0
                return (
                  <tr key={monitor.id}>
                    <td>
                      <Link className={styles.nameCell} to={`/monitors/${monitor.id}`}>
                        <strong>{monitor.name}</strong>
                        <span>{monitor.target}</span>
                      </Link>
                    </td>
                    <td>
                      <Badge tone={kindTone(monitor.kind)}>{kindLabel[monitor.kind]}</Badge>
                    </td>
                    <td>{monitor.target}</td>
                    <td>
                      <Badge tone={currentStatus === 'success' ? 'green' : currentStatus === 'failed' ? 'red' : 'gray'}>
                        {statusLabel[currentStatus]}
                      </Badge>
                    </td>
                    <td>{latencyMs(result?.latency_us)}</td>
                    <td>
                      <span>{percent.toFixed(2)}% </span>
                      <span className={styles.bar}>
                        <span style={{ width: `${percent}%` }} />
                      </span>
                    </td>
                    <td>{compactTime(result?.checked_at)}</td>
                    <td>{Math.round(monitor.interval_seconds / 60) || monitor.interval_seconds} 分钟</td>
                    <td>
                      <Switch
                        label={`${monitor.enabled ? '暂停' : '启用'} ${monitor.name}`}
                        checked={monitor.enabled}
                        disabled={toggleMutation.isPending}
                        onChange={() => toggleMutation.mutate(monitor)}
                      />
                    </td>
                    <td>
                      <div className={styles.filterGroup}>
                        <button
                          className={styles.iconButton}
                          type="button"
                          title={monitor.enabled ? '暂停' : '恢复'}
                          onClick={() => toggleMutation.mutate(monitor)}
                        >
                          {monitor.enabled ? <Pause size={15} /> : <Play size={15} />}
                        </button>
                        <Link className={styles.iconButton} title="编辑" to={`/monitors/${monitor.id}/edit`}>
                          <Edit3 size={15} />
                        </Link>
                        <button
                          className={styles.iconButton}
                          type="button"
                          title="删除"
                          onClick={() => {
                            if (window.confirm(`确认删除监控项「${monitor.name}」？`)) {
                              deleteMutation.mutate(monitor.id)
                            }
                          }}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>

        {!rows.length && !dashboard.isLoading ? <EmptyState title="没有匹配的监控项" /> : null}
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

      {dashboard.isError ? <div className={styles.error}>监控项加载失败，请确认后端服务已启动。</div> : null}
    </div>
  )
}
