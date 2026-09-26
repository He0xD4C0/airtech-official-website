import type { ApiProblem } from '@/shared/types/domain'
import { adminApiBaseUrl } from '@/app/runtimeConfig'
import { getAdminCsrfToken } from '@/shared/services/adminCsrf'

function developmentApiBaseUrl(): string {
  return adminApiBaseUrl().replace(/\/api\/admin\/v1$/, '/api/devtools/v1')
}

export async function createDevtoolsTerminalToken(): Promise<{ token: string; expiresInSeconds: number }> {
  if (!__AIRTEK_DEVTOOLS__) throw new Error('DevTools are not available in this build.')
  const headers = new Headers({ Accept: 'application/json' })
  const csrfToken = getAdminCsrfToken()
  if (csrfToken) headers.set('X-CSRF-Token', csrfToken)
  const response = await fetch(`${developmentApiBaseUrl()}/sessions/token`, {
    method: 'POST',
    credentials: 'include',
    headers,
  })
  if (!response.ok) {
    const problem = await response.json().catch(() => null) as ApiProblem | null
    throw new Error(problem?.detail ?? problem?.title ?? 'Unable to authorize the development terminal.')
  }
  return response.json() as Promise<{ token: string; expiresInSeconds: number }>
}

export function devtoolsTerminalUrl(token: string): string {
  const url = new URL(`${developmentApiBaseUrl()}/terminal`)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('token', token)
  return url.toString()
}
