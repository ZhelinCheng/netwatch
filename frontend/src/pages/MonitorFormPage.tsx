import { Save } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Switch } from '../components/Switch'
import { netwatchApi } from '../api/netwatch'
import type { DnsRecordType, HeaderMatchMode, HttpHeaderMatch, MonitorKind, MonitorPayload } from '../api/types'
import styles from './pages.module.scss'

const dnsRecordTypes: DnsRecordType[] = ['A', 'AAAA', 'CNAME', 'MX', 'TXT', 'NS', 'SOA', 'CAA', 'SRV']
const defaultName = '我的网站监控'
const defaultKind: MonitorKind = 'http'
const defaultTarget = 'https://www.example.com'
const defaultInterval = 60
const defaultTimeout = 10
const defaultStatusMin = 200
const defaultStatusMax = 399

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

  const [name, setName] = useState(defaultName)
  const [kind, setKind] = useState<MonitorKind>(defaultKind)
  const [target, setTarget] = useState(defaultTarget)
  const [enabled, setEnabled] = useState(true)
  const [interval, setInterval] = useState(defaultInterval)
  const [timeout, setTimeoutValue] = useState(defaultTimeout)
  const [statusMin, setStatusMin] = useState(defaultStatusMin)
  const [statusMax, setStatusMax] = useState(defaultStatusMax)
  const [keyword, setKeyword] = useState('')
  const [headerMatchMode, setHeaderMatchMode] = useState<HeaderMatchMode>('all')
  const [headerLines, setHeaderLines] = useState('')
  const [dnsRecord, setDnsRecord] = useState<DnsRecordType>('A')
  const [expectedValue, setExpectedValue] = useState('')
  const [error, setError] = useState('')
  const [loadedFormKey, setLoadedFormKey] = useState<string | null>(null)

  useEffect(() => {
    if (monitorQuery.data) {
      const formKey = `monitor:${monitorQuery.data.id}`
      if (loadedFormKey === formKey) return
      setName(monitorQuery.data.name)
      setKind(monitorQuery.data.kind)
      setTarget(monitorQuery.data.target)
      setEnabled(monitorQuery.data.enabled)
      setInterval(monitorQuery.data.interval_seconds)
      setTimeoutValue(monitorQuery.data.timeout_seconds)
      setStatusMin(monitorQuery.data.config.expected_status_min ?? defaultStatusMin)
      setStatusMax(monitorQuery.data.config.expected_status_max ?? defaultStatusMax)
      setKeyword(monitorQuery.data.config.keyword ?? '')
      setHeaderMatchMode(monitorQuery.data.config.header_match_mode ?? 'all')
      setHeaderLines(headersToLines(monitorQuery.data.config.expected_headers))
      setDnsRecord(normalizeDnsRecord(monitorQuery.data.config.dns_record))
      setExpectedValue(monitorQuery.data.config.expected_value ?? '')
      setLoadedFormKey(formKey)
      return
    }

    if (!isEdit && loadedFormKey !== 'new') {
      setName(defaultName)
      setKind(defaultKind)
      setTarget(defaultTarget)
      setEnabled(true)
      setInterval(defaultInterval)
      setTimeoutValue(defaultTimeout)
      setStatusMin(defaultStatusMin)
      setStatusMax(defaultStatusMax)
      setKeyword('')
      setHeaderMatchMode('all')
      setHeaderLines('')
      setDnsRecord('A')
      setExpectedValue('')
      setLoadedFormKey('new')
    }
  }, [isEdit, loadedFormKey, monitorQuery.data])

  const formName = name
  const formKind = kind
  const formTarget = target
  const formEnabled = enabled
  const formInterval = interval
  const formTimeout = timeout
  const formStatusMin = statusMin
  const formStatusMax = statusMax
  const formKeyword = keyword
  const formHeaderMatchMode = headerMatchMode
  const formHeaderLines = headerLines
  const formDnsRecord = dnsRecord
  const formExpectedValue = expectedValue
  const parsedHeaders = useMemo(() => parseHeaderLines(formHeaderLines), [formHeaderLines])

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
        expected_headers: formKind === 'http' ? parsedHeaders.headers : null,
        header_match_mode: formKind === 'http' ? formHeaderMatchMode : null,
        dns_record: formKind === 'dns' ? formDnsRecord : null,
        expected_value: formKind === 'dns' && formExpectedValue.trim() ? formExpectedValue.trim() : null,
      },
    }),
    [
      formDnsRecord,
      formEnabled,
      formExpectedValue,
      formInterval,
      formHeaderMatchMode,
      formKeyword,
      formKind,
      formHeaderLines,
      formName,
      formStatusMax,
      formStatusMin,
      formTarget,
      formTimeout,
      parsedHeaders.headers,
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
    if (formInterval < 5) {
      setError('检查间隔至少为 5 秒')
      return
    }
    if (formTimeout <= 0 || formTimeout > formInterval) {
      setError('超时时间必须大于 0，且不能大于检查间隔')
      return
    }
    if (formKind === 'http' && parsedHeaders.error) {
      setError(parsedHeaders.error)
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
          <p>配置目标、检查间隔和协议探测条件。</p>
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
              <input className={styles.input} type="number" min={5} value={formInterval} onChange={(event) => update(setInterval, Number(event.target.value))} />
              <small>最小 5 秒</small>
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
                <span>响应关键词/正则（可选）</span>
                <input className={styles.input} value={formKeyword} placeholder="例如：healthy|ok" onChange={(event) => update(setKeyword, event.target.value)} />
                <small>按正则匹配响应体；普通文本会作为包含匹配使用。</small>
              </label>
              <div className={styles.formGrid}>
                <label className={styles.field}>
                  <span>响应头匹配模式</span>
                  <select className={styles.select} value={formHeaderMatchMode} onChange={(event) => update(setHeaderMatchMode, event.target.value as HeaderMatchMode)}>
                    <option value="all">全部满足</option>
                    <option value="any">任一满足</option>
                  </select>
                </label>
                <label className={styles.field}>
                  <span>响应头匹配（可选）</span>
                  <textarea
                    className={styles.textarea}
                    value={formHeaderLines}
                    placeholder={'content-type: application/json\nx-env: prod|staging'}
                    onChange={(event) => update(setHeaderLines, event.target.value)}
                  />
                  <small>每行一个 key: 正则表达式。</small>
                </label>
              </div>
            </>
          ) : null}

          {formKind === 'dns' ? (
            <div className={styles.formGrid}>
              <label className={styles.field}>
                <span>DNS 记录类型</span>
                <select className={styles.select} value={formDnsRecord} onChange={(event) => update(setDnsRecord, event.target.value as DnsRecordType)}>
                  {dnsRecordTypes.map((recordType) => (
                    <option key={recordType} value={recordType}>
                      {recordType}
                    </option>
                  ))}
                </select>
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

    </form>
  )

  function update<T>(setter: (value: T) => void, value: T) {
    setter(value)
  }
}

function normalizeDnsRecord(value?: DnsRecordType | string | null): DnsRecordType {
  return dnsRecordTypes.includes(value as DnsRecordType) ? (value as DnsRecordType) : 'A'
}

function headersToLines(headers?: HttpHeaderMatch[] | null) {
  return headers?.map((header) => `${header.key}: ${header.value}`).join('\n') ?? ''
}

function parseHeaderLines(lines: string): { headers: HttpHeaderMatch[] | null; error: string } {
  const headers: HttpHeaderMatch[] = []
  for (const line of lines.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed) continue
    const separatorIndex = trimmed.indexOf(':')
    if (separatorIndex <= 0) {
      return { headers: null, error: '响应头匹配每行必须使用 key: 正则表达式 格式' }
    }
    const key = trimmed.slice(0, separatorIndex).trim()
    const value = trimmed.slice(separatorIndex + 1).trim()
    if (!key || !value) {
      return { headers: null, error: '响应头匹配的 key 和正则表达式不能为空' }
    }
    headers.push({ key, value })
  }
  return { headers: headers.length ? headers : null, error: '' }
}
