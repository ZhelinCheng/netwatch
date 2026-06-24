import {
  Activity,
  Bell,
  ChevronRight,
  Grid2X2,
  Menu,
  MonitorCheck,
  Search,
  Settings,
  ShieldCheck,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { netwatchApi } from '../api/netwatch'
import styles from './Layout.module.scss'

const navItems = [
  { to: '/dashboard', label: '总览', icon: Grid2X2 },
  { to: '/monitors', label: '监控项', icon: MonitorCheck },
  { to: '/alerts', label: '告警', icon: Bell },
  { to: '/status', label: '状态页', icon: Activity },
  { to: '/settings', label: '设置', icon: Settings },
]

const alertsViewedKey = 'netwatch.alerts.viewedAt'

const titles: Record<string, string> = {
  '/dashboard': '总览 Dashboard',
  '/monitors': '监控项',
  '/monitors/new': '监控项 / 新建监控项',
  '/alerts': '告警',
  '/status': '状态页',
  '/settings': '设置',
}

interface LayoutProps {
  children?: ReactNode
}

export function Layout({ children }: LayoutProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const [searchText, setSearchText] = useState('')
  const [lastViewedAlertAt, setLastViewedAlertAt] = useState(() => {
    const stored = window.localStorage.getItem(alertsViewedKey)
    const timestamp = stored ? Number(stored) : 0
    return Number.isFinite(timestamp) ? timestamp : 0
  })
  const title =
    titles[location.pathname] ??
    (location.pathname.startsWith('/monitors/') ? '监控项 / 监控详情' : 'Netwatch')
  const alerts = useQuery({
    queryKey: ['alerts'],
    queryFn: () => netwatchApi.alerts(500),
    refetchInterval: 30_000,
  })
  const latestAlertAt = useMemo(
    () => Math.max(0, ...(alerts.data ?? []).map((alert) => alert.created_at)),
    [alerts.data],
  )
  const unreadTriggeredCount = useMemo(
    () =>
      (alerts.data ?? []).filter(
        (alert) => alert.kind === 'triggered' && alert.created_at > lastViewedAlertAt,
      ).length,
    [alerts.data, lastViewedAlertAt],
  )
  const showAlertCount = location.pathname !== '/alerts' && unreadTriggeredCount > 0

  function submitSearch() {
    const keyword = searchText.trim()
    navigate(keyword ? `/monitors?keyword=${encodeURIComponent(keyword)}` : '/monitors')
  }

  useEffect(() => {
    if (location.pathname !== '/alerts' || !latestAlertAt) {
      return
    }

    setLastViewedAlertAt(latestAlertAt)
    window.localStorage.setItem(alertsViewedKey, String(latestAlertAt))
  }, [latestAlertAt, location.pathname])

  return (
    <div className={styles.shell}>
      <aside className={styles.sidebar}>
        <div className={styles.brand}>
          <ShieldCheck size={30} />
          <strong>Netwatch</strong>
        </div>

        <nav className={styles.nav}>
          {navItems.map((item) => {
            const Icon = item.icon
            return (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) => `${styles.navItem} ${isActive ? styles.active : ''}`}
              >
                <Icon size={18} />
                <span>{item.label}</span>
                {item.to === '/alerts' && showAlertCount ? <b>{unreadTriggeredCount}</b> : null}
              </NavLink>
            )
          })}
        </nav>

        <div className={styles.version}>
          <span />
          <div>
            <strong>Netwatch</strong>
            <small>v1.6.2</small>
          </div>
          <ChevronRight size={16} />
        </div>
      </aside>

      <main className={styles.main}>
        <header className={styles.topbar}>
          <div className={styles.mobileTitle}>
            <Menu size={20} />
            <span>{title}</span>
          </div>
          <div className={styles.search}>
            <Search size={16} />
            <input
              placeholder="搜索监控项、目标或标签..."
              value={searchText}
              onChange={(event) => setSearchText(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') submitSearch()
              }}
            />
            <button type="button" onClick={submitSearch}>
              搜索
            </button>
          </div>
        </header>
        <section className={styles.content}>{children ?? <Outlet />}</section>
      </main>
    </div>
  )
}
