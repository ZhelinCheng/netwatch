import { apiRequest, withQuery } from './client'
import type { AlertEvent, ChecksResponse, Dashboard, Monitor, MonitorPayload } from './types'

export const netwatchApi = {
  dashboard: () => apiRequest<Dashboard>('/api/dashboard'),
  monitors: () => apiRequest<Monitor[]>('/api/monitors'),
  monitor: (id: number) => apiRequest<Monitor>(`/api/monitors/${id}`),
  createMonitor: (payload: MonitorPayload) =>
    apiRequest<Monitor>('/api/monitors', { method: 'POST', body: payload }),
  updateMonitor: (id: number, payload: Partial<MonitorPayload>) =>
    apiRequest<Monitor>(`/api/monitors/${id}`, { method: 'PATCH', body: payload }),
  deleteMonitor: (id: number) => apiRequest<void>(`/api/monitors/${id}`, { method: 'DELETE' }),
  pauseMonitor: (id: number) =>
    apiRequest<Monitor>(`/api/monitors/${id}/pause`, { method: 'POST' }),
  resumeMonitor: (id: number) =>
    apiRequest<Monitor>(`/api/monitors/${id}/resume`, { method: 'POST' }),
  checks: (id: number, params: { limit?: number; from?: number; to?: number }) =>
    apiRequest<ChecksResponse>(withQuery(`/api/monitors/${id}/checks`, params)),
  alerts: (limit = 500) => apiRequest<AlertEvent[]>(withQuery('/api/alerts', { limit })),
}
