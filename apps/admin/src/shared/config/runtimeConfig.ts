export interface AdminRuntimeConfig {
  apiBaseUrl: string
}

const developmentApiBaseUrl = 'http://localhost:8080/api/admin/v1'
let activeConfig: AdminRuntimeConfig = {
  apiBaseUrl: normalizeAdminApiBaseUrl(import.meta.env.VITE_ADMIN_API_BASE_URL ?? developmentApiBaseUrl),
}

function normalizeAdminApiBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)
    || url.username
    || url.password
    || url.search
    || url.hash
    || url.pathname.replace(/\/$/u, '') !== '/api/admin/v1') {
    throw new Error('Admin runtime apiBaseUrl must be an absolute HTTP(S) /api/admin/v1 URL.')
  }
  return `${url.origin}/api/admin/v1`
}

export function configureAdminRuntime(value: AdminRuntimeConfig): void {
  activeConfig = { apiBaseUrl: normalizeAdminApiBaseUrl(value.apiBaseUrl) }
}

export async function initializeAdminRuntime(fetchImpl: typeof fetch = fetch): Promise<void> {
  if (!import.meta.env.PROD) return
  const response = await fetchImpl('/runtime-config.json', {
    cache: 'no-store',
    credentials: 'same-origin',
    headers: { Accept: 'application/json' },
  })
  if (!response.ok) throw new Error(`Admin runtime configuration returned HTTP ${response.status}.`)
  const value = await response.json() as Partial<AdminRuntimeConfig>
  if (typeof value.apiBaseUrl !== 'string') throw new Error('Admin runtime configuration is incomplete.')
  configureAdminRuntime({ apiBaseUrl: value.apiBaseUrl })
}

export function adminApiBaseUrl(): string {
  return activeConfig.apiBaseUrl
}

export function adminApiOrigin(): string {
  return new URL(activeConfig.apiBaseUrl).origin
}
