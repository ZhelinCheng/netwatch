import {
  Activity,
  Bell,
  CalendarDays,
  ChevronRight,
  Grid2X2,
  Menu,
  MonitorCheck,
  RefreshCcw,
  Search,
  Settings,
  ShieldCheck,
} from 'lucide-react'
import { useState } from 'react'
import type { ReactNode } from 'react'
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import styles from './Layout.module.scss'

const navItems = [
  { to: '/dashboard', label: '总览', icon: Grid2X2 },
  { to: '/monitors', label: '监控项', icon: MonitorCheck },
  { to: '/alerts', label: '告警', icon: Bell, count: 2 },
  { to: '/status', label: '状态页', icon: Activity },
  { to: '/settings', label: '设置', icon: Settings },
]

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
  const title =
    titles[location.pathname] ??
    (location.pathname.startsWith('/monitors/') ? '监控项 / 监控详情' : 'Netwatch')

  function submitSearch() {
    const keyword = searchText.trim()
    navigate(keyword ? `/monitors?keyword=${encodeURIComponent(keyword)}` : '/monitors')
  }

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
                {item.count ? <b>{item.count}</b> : null}
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
