import path from 'node:path'

const gatewayPort = process.env.AIRTEK_GATEWAY_HOST_PORT ?? '8088'

export const runAdminWorkflows = process.env.E2E_RUN_ADMIN_WORKFLOWS === 'true'
export const isolatedStack = process.env.E2E_ISOLATED_STACK === 'true'
export const publicOrigin = process.env.E2E_PUBLIC_ORIGIN
  ?? `http://www.airtek.test:${gatewayPort}`
export const adminOrigin = process.env.E2E_ADMIN_ORIGIN
  ?? `http://admin.airtek.test:${gatewayPort}`
export const apiOrigin = process.env.E2E_API_ORIGIN
  ?? `http://api.airtek.test:${gatewayPort}`
export const apiControlOrigin = process.env.E2E_API_CONTROL_ORIGIN ?? apiOrigin
export const adminStorageStatePath = process.env.E2E_ADMIN_STORAGE_STATE
  ?? path.resolve('test-results/playwright/e2e-admin-storage-state.json')

export const administrator = {
  displayName: 'AIRTEK E2E Administrator',
  email: process.env.E2E_ADMIN_EMAIL ?? 'e2e-admin@airtek.invalid',
  password: process.env.E2E_ADMIN_PASSWORD ?? 'Airtek-E2E-Admin-123!',
  bootstrapToken: process.env.AIRTEK_ADMIN_BOOTSTRAP_TOKEN
    ?? 'airtek-e2e-bootstrap-token-change-me',
}

export const restrictedUser = {
  displayName: 'AIRTEK E2E Restricted Editor',
  email: process.env.E2E_RESTRICTED_EMAIL ?? 'e2e-restricted@airtek.invalid',
  password: process.env.E2E_RESTRICTED_PASSWORD ?? 'Airtek-E2E-Restricted-123!',
}

export function absolute(origin: string, pathname: string): string {
  return new URL(pathname, `${origin.replace(/\/$/u, '')}/`).toString()
}

export function apiControlHeaders(): Record<string, string> {
  return {
    Origin: adminOrigin,
    ...(new URL(apiControlOrigin).host === new URL(apiOrigin).host ? {} : { Host: new URL(apiOrigin).host }),
  }
}

export function browserCookiesForLocalGateway<T extends { domain: string; secure: boolean }>(cookies: T[]): T[] {
  const apiHostname = new URL(apiOrigin).hostname
  return cookies.map((cookie) => ({
    ...cookie,
    domain: apiHostname,
    secure: apiOrigin.startsWith('https://') ? cookie.secure : false,
  }))
}
