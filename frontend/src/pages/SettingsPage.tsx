import { Copy, EyeOff, Info, Lightbulb } from 'lucide-react'
import styles from './pages.module.scss'

const groups = [
  {
    title: '服务配置',
    rows: [
      ['NETWATCH_HOST', '服务监听地址', '127.0.0.1'],
      ['NETWATCH_PORT', '服务监听端口', '4311'],
      ['NETWATCH_DATABASE_URL', '数据库连接地址（SQLite）', 'sqlite://./data/netwatch.db'],
    ],
  },
  {
    title: '调度配置',
    rows: [
      ['NETWATCH_SCHEDULER_TICK_SECONDS', '调度扫描周期（秒）', '5'],
      ['NETWATCH_FAILURE_THRESHOLD', '告警触发阈值（连续失败次数）', '3'],
    ],
  },
  {
    title: '数据保留与聚合',
    rows: [
      ['NETWATCH_AGGREGATION_TIMEZONE', '聚合时区', 'Asia/Shanghai'],
      ['NETWATCH_CHECK_FLUSH_INTERVAL_SECONDS', '检查结果刷盘间隔（秒）', '60'],
      ['NETWATCH_COMPACT_INTERVAL_SECONDS', '数据聚合与清理间隔（秒）', '300'],
      ['NETWATCH_RAW_DATA_RETENTION_DAYS', '原始数据保留天数', '7'],
    ],
  },
  {
    title: '通知配置',
    rows: [
      ['NETWATCH_WEBHOOK_URL', '告警 Webhook 地址', '未暴露'],
      ['NETWATCH_WEBHOOK_TIMEOUT_SECONDS', 'Webhook 超时时间（秒）', '10'],
    ],
  },
]

export function SettingsPage() {
  return (
    <div className={styles.page}>
      <div className={styles.pageHeader}>
        <div>
          <h1>设置</h1>
          <p>当前页面展示开发环境下可维护的 Netwatch 环境变量说明。</p>
        </div>
      </div>

      <section className={styles.info}>
        <Info size={20} />
        <div>
          <strong>配置来自环境变量，修改后需重启服务生效</strong>
          <p>当前显示的是服务启动时读取的配置范围；本版本不新增后端配置查询接口。</p>
        </div>
      </section>

      {groups.map((group) => (
        <section className={styles.card} key={group.title}>
          <div className={styles.cardHeader}>
            <h2>{group.title}</h2>
          </div>
          <div className={styles.configRows}>
            {group.rows.map(([key, label, value]) => (
              <div className={styles.configRow} key={key}>
                <code>{key}</code>
                <span>{label}</span>
                <strong>
                  {value === '未暴露' ? <EyeOff size={15} /> : null} {value}
                </strong>
                <button className={styles.iconButton} type="button" title="复制配置名" onClick={() => navigator.clipboard?.writeText(key)}>
                  <Copy size={15} />
                </button>
              </div>
            ))}
          </div>
        </section>
      ))}

      <section className={styles.info}>
        <Lightbulb size={20} />
        <div>
          <strong>更多配置请通过环境变量调整</strong>
          <p>部署环境可使用 systemd、Docker Compose 或平台变量管理这些值。</p>
        </div>
      </section>
    </div>
  )
}
