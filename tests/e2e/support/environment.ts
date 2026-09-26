import path from 'node:path'

const gatewayPort = process.env.AIRTEK_GATEWAY_HOST_PORT ?? '8088'

export const runAdminWorkflows = process.env.E2E_RUN_ADMIN_WORKFLOWS === 'true'
export const isolatedStack = process.env.E2E_ISOLATED_STACK === 'true'
export const publicOrigin = process.env.E2E_PUBLIC_ORIGIN
  ?? `http://www.airtek.localhost:${gatewayPort}`
export const adminOrigin = process.env.E2E_ADMIN_ORIGIN
  ?? `http://admin.airtek.localhost:${gatewayPort}`
export const apiOrigin = process.env.E2E_API_ORIGIN
  ?? `http://api.airtek.localhost:${gatewayPort}`
export const apiControlOrigin = process.env.E2E_API_CONTROL_ORIGIN ?? apiOrigin
export const gatewayControlOrigin = process.env.E2E_GATEWAY_CONTROL_ORIGIN ?? publicOrigin
export const mediaPublicBaseUrl = process.env.E2E_MEDIA_PUBLIC_BASE_URL
  ?? 'http://media.localhost:19000/airtek-media'
export const adminStorageStatePath = process.env.E2E_ADMIN_STORAGE_STATE
  ?? path.resolve('.local/qa/test-results/playwright/e2e-admin-storage-state.json')
export const adminSecondaryStorageStatePath = process.env.E2E_ADMIN_SECONDARY_STORAGE_STATE
  ?? path.resolve('.local/qa/test-results/playwright/e2e-admin-secondary-storage-state.json')
export const adminTotpSecretPath = process.env.E2E_ADMIN_TOTP_SECRET
  ?? path.resolve('.local/qa/test-results/playwright/e2e-admin-totp-secret.txt')

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

export function gatewayHostHeaders(origin: string): Record<string, string> {
  return new URL(gatewayControlOrigin).host === new URL(origin).host
    ? {}
    : { Host: new URL(origin).host }
}

export function browserCookiesForLocalGateway<T extends { domain: string; secure: boolean }>(cookies: T[]): T[] {
  const apiHostname = new URL(apiOrigin).hostname
  return cookies.map((cookie) => ({
    ...cookie,
    domain: apiHostname,
    secure: cookie.secure,
  }))
}
