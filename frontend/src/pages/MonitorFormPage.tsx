import { Plus, Save, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Switch } from '../components/Switch'
import { netwatchApi } from '../api/netwatch'
import type { MonitorKind, MonitorPayload, SuccessRule } from '../api/types'
import styles from './pages.module.scss'

interface RuleDraft {
  metric: 'status' | 'latency' | 'body'
  op: 'gte' | 'lte' | 'contains'
  value: string
}

const defaultRules: RuleDraft[] = [
  { metric: 'status', op: 'gte', value: '200' },
  { metric: 'status', op: 'lte', value: '399' },
  { metric: 'latency', op: 'lte', value: '2000' },
]

export function MonitorFormPage() {
  const { id } = useParams()
  const numericId = id ? Number(id) : undefined
  const isEdit = Number.isFinite(numericId)
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const monitorQuery = useQuery({
    queryKey: ['monitor', numericId],
    enabled: isEdit,
    queryFn: () => netwatchApi.monitor(numericId!),
  })

  const [name, setName] = useState('我的网站监控')
  const [kind, setKind] = useState<MonitorKind>('http')
  const [target, setTarget] = useState('https://www.example.com')
  const [enabled, setEnabled] = useState(true)
  const [interval, setInterval] = useState(60)
  const [timeout, setTimeoutValue] = useState(10)
  const [statusMin, setStatusMin] = useState(200)
  const [statusMax, setStatusMax] = useState(399)
  const [keyword, setKeyword] = useState('')
  const [dnsRecord, setDnsRecord] = useState('A')
  const [expectedValue, setExpectedValue] = useState('')
  const [rules, setRules] = useState<RuleDraft[]>(defaultRules)
  const [error, setError] = useState('')
  const [dirty, setDirty] = useState(false)

  const formName = !dirty && monitorQuery.data ? monitorQuery.data.name : name
  const formKind = !dirty && monitorQuery.data ? monitorQuery.data.kind : kind
  const formTarget = !dirty && monitorQuery.data ? monitorQuery.data.target : target
  const formEnabled = !dirty && monitorQuery.data ? monitorQuery.data.enabled : enabled
  const formInterval = !dirty && monitorQuery.data ? monitorQuery.data.interval_seconds : interval
  const formTimeout = !dirty && monitorQuery.data ? monitorQuery.data.timeout_seconds : timeout
  const formStatusMin = !dirty && monitorQuery.data ? (monitorQuery.data.config.expected_status_min ?? 200) : statusMin
  const formStatusMax = !dirty && monitorQuery.data ? (monitorQuery.data.config.expected_status_max ?? 399) : statusMax
  const formKeyword = !dirty && monitorQuery.data ? (monitorQuery.data.config.keyword ?? '') : keyword
  const formDnsRecord = !dirty && monitorQuery.data ? (monitorQuery.data.config.dns_record ?? 'A') : dnsRecord
  const formExpectedValue =
    !dirty && monitorQuery.data ? (monitorQuery.data.config.expected_value ?? '') : expectedValue

  const mutation = useMutation({
    mutationFn: (payload: MonitorPayload) =>
      isEdit ? netwatchApi.updateMonitor(numericId!, payload) : netwatchApi.createMonitor(payload),
    onSuccess: (monitor) => {
      queryClient.invalidateQueries({ queryKey: ['dashboard'] })
      queryClient.invalidateQueries({ queryKey: ['monitor', monitor.id] })
      navigate(`/monitors/${monitor.id}`)
    },
  })

  const payload = useMemo<MonitorPayload>(
    () => ({
      name: formName.trim(),
      kind: formKind,
      target: formTarget.trim(),
      enabled: formEnabled,
      interval_seconds: formInterval,
      timeout_seconds: formTimeout,
      config: {
        expected_status_min: formKind === 'http' ? formStatusMin : null,
        expected_status_max: formKind === 'http' ? formStatusMax : null,
        keyword: formKind === 'http' && formKeyword.trim() ? formKeyword.trim() : null,
        dns_record: formKind === 'dns' ? formDnsRecord : null,
        expected_value: formKind === 'dns' && formExpectedValue.trim() ? formExpectedValue.trim() : null,
        success_rules: rulesToPayload(rules, formKind),
      },
    }),
    [
      formDnsRecord,
      formEnabled,
      formExpectedValue,
      formInterval,
      formKeyword,
      formKind,
      formName,
      formStatusMax,
      formStatusMin,
      formTarget,
      formTimeout,
      rules,
    ],
  )

  function submit(event: FormEvent) {
    event.preventDefault()
    if (!payload.name) {
      setError('名称不能为空')
      return
    }
    if (!payload.target) {
      setError('目标地址不能为空')
      return
    }
    if (formInterval < 2) {
      setError('检查间隔至少为 2 秒')
      return
    }
    if (formTimeout <= 0 || formTimeout > formInterval) {
      setError('超时时间必须大于 0，且不能大于检查间隔')
      return
    }
    setError('')
    mutation.mutate(payload)
  }

  return (
    <form className={styles.form} onSubmit={submit}>
      <div className={styles.pageHeader}>
        <div>
          <h1>{isEdit ? '编辑监控项' : '新建监控项'}</h1>
          <p>配置目标、检查间隔和成功规则。</p>
        </div>
        <div className={styles.filterGroup}>
          <Link className={styles.ghostButton} to={isEdit ? `/monitors/${numericId}` : '/monitors'}>
            取消
          </Link>
          <button className={styles.button} type="submit" disabled={mutation.isPending}>
            <Save size={16} /> 保存监控项
          </button>
        </div>
      </div>

      {error || mutation.isError ? (
        <div className={styles.error}>{error || '保存失败，请检查输入或后端服务状态。'}</div>
      ) : null}

      <section className={styles.section}>
        <div className={styles.cardHeader}>
          <h2>基础信息</h2>
          <Switch checked={formEnabled} label="启用监控项" onChange={(value) => update(setEnabled, value)} />
        </div>
        <div className={styles.sectionBody}>
          <div className={styles.formGrid}>
            <label className={styles.field}>
              <span>名称 *</span>
              <input className={styles.input} value={formName} onChange={(event) => update(setName, event.target.value)} />
            </label>
            <label className={styles.field}>
              <span>目标地址 *</span>
              <input className={styles.input} value={formTarget} onChange={(event) => update(setTarget, event.target.value)} />
              <small>{formKind === 'http' ? '请输入完整 URL，包含 http:// 或 https://' : '请输入域名、IP 或 host:port'}</small>
            </label>
          </div>

          <div className={styles.field}>
            <label>类型</label>
            <div className={styles.segmented}>
              {(['http', 'dns', 'tcp', 'ping'] as MonitorKind[]).map((item) => (
                <button
                  type="button"
                  key={item}
                  className={item === formKind ? styles.segmentActive : ''}
                  onClick={() => update(setKind, item)}
                >
                  {item.toUpperCase()}
                </button>
              ))}
            </div>
          </div>

          <div className={styles.formGrid}>
            <label className={styles.field}>
              <span>检查间隔（秒） *</span>
              <input className={styles.input} type="number" min={2} value={formInterval} onChange={(event) => update(setInterval, Number(event.target.value))} />
              <small>最小 2 秒</small>
            </label>
            <label className={styles.field}>
              <span>超时时间（秒） *</span>
              <input className={styles.input} type="number" min={1} value={formTimeout} onChange={(event) => update(setTimeoutValue, Number(event.target.value))} />
              <small>必须大于 0，且小于等于检查间隔</small>
            </label>
          </div>
        </div>
      </section>

      <section className={styles.section}>
        <div className={styles.cardHeader}>
          <h2>探测配置（{formKind.toUpperCase()}）</h2>
        </div>
        <div className={styles.sectionBody}>
          {formKind === 'http' ? (
            <>
              <div className={styles.formGrid}>
                <label className={styles.field}>
                  <span>期望状态码范围 *</span>
                  <input className={styles.input} type="number" value={formStatusMin} onChange={(event) => update(setStatusMin, Number(event.target.value))} />
                </label>
                <label className={styles.field}>
                  <span>状态码上限 *</span>
                  <input className={styles.input} type="number" value={formStatusMax} onChange={(event) => update(setStatusMax, Number(event.target.value))} />
                </label>
              </div>
              <label className={styles.field}>
                <span>响应关键词（可选）</span>
                <input className={styles.input} value={formKeyword} placeholder="例如：success, ok, healthy" onChange={(event) => update(setKeyword, event.target.value)} />
              </label>
            </>
          ) : null}

          {formKind === 'dns' ? (
            <div className={styles.formGrid}>
              <label className={styles.field}>
                <span>DNS 记录类型</span>
                <input className={styles.input} value={formDnsRecord} onChange={(event) => update(setDnsRecord, event.target.value)} />
              </label>
              <label className={styles.field}>
                <span>期望解析值（可选）</span>
                <input className={styles.input} value={formExpectedValue} onChange={(event) => update(setExpectedValue, event.target.value)} />
              </label>
            </div>
          ) : null}

          {formKind === 'tcp' || formKind === 'ping' ? (
            <div className={styles.info}>
              <span>i</span>
              <div>
                <strong>当前类型使用通用探测参数</strong>
                <p>目标、检查间隔与超时时间会直接传递给对应探测器。</p>
              </div>
            </div>
          ) : null}
        </div>
      </section>

      <section className={styles.section}>
        <div className={styles.cardHeader}>
          <h2>高级成功规则（可选）</h2>
          <button className={styles.ghostButton} type="button" onClick={() => setRules([...rules, { metric: 'body', op: 'contains', value: '' }])}>
            <Plus size={16} /> 添加规则
          </button>
        </div>
        <div className={styles.sectionBody}>
          <div className={styles.rules}>
            {rules.map((rule, index) => (
              <div className={styles.ruleRow} key={`${rule.metric}-${index}`}>
                <select className={styles.select} value={rule.metric} onChange={(event) => updateRule(index, { metric: event.target.value as RuleDraft['metric'] })}>
                  <option value="status">状态码</option>
                  <option value="latency">响应时间（ms）</option>
                  <option value="body">响应体包含</option>
                </select>
                <select className={styles.select} value={rule.op} onChange={(event) => updateRule(index, { op: event.target.value as RuleDraft['op'] })}>
                  <option value="gte">大于等于</option>
                  <option value="lte">小于等于</option>
                  <option value="contains">包含</option>
                </select>
                <input className={styles.input} value={rule.value} onChange={(event) => updateRule(index, { value: event.target.value })} />
                <button className={styles.iconButton} type="button" onClick={() => setRules(rules.filter((_, itemIndex) => itemIndex !== index))}>
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
          </div>
        </div>
      </section>
    </form>
  )

  function updateRule(index: number, patch: Partial<RuleDraft>) {
    setDirty(true)
    setRules(rules.map((rule, itemIndex) => (itemIndex === index ? { ...rule, ...patch } : rule)))
  }

  function update<T>(setter: (value: T) => void, value: T) {
    setDirty(true)
    setter(value)
  }
}

function rulesToPayload(rules: RuleDraft[], kind: MonitorKind): SuccessRule[] | null {
  if (kind !== 'http') return null
  const payload = rules.flatMap<SuccessRule>((rule) => {
    if (!rule.value.trim()) return []
    if (rule.metric === 'status') {
      return [{ type: 'http_status', op: rule.op === 'gte' ? 'gte' : 'lte', value: Number(rule.value) }]
    }
    if (rule.metric === 'latency') {
      return [{ type: 'latency', op: 'lte', value_us: Number(rule.value) * 1000 }]
    }
    return [{ type: 'http_body', op: 'contains', value: rule.value.trim() }]
  })
  return payload.length ? payload : null
}
