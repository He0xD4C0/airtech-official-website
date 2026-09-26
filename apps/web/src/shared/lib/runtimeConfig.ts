import { normalizePublicOrigin } from '@/shared/lib/publicOrigin'

export interface PublicRuntimeConfig {
  apiBaseUrl: string
  publicOrigin: string
}

let activeConfig: PublicRuntimeConfig = {
  apiBaseUrl: normalizePublicApiBaseUrl(import.meta.env.VITE_PUBLIC_API_BASE_URL ?? 'http://localhost:8080/api/public/v1'),
  publicOrigin: normalizePublicOrigin(import.meta.env.VITE_PUBLIC_ORIGIN),
}

function normalizePublicApiBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)
    || url.username
    || url.password
    || url.search
    || url.hash
    || url.pathname.replace(/\/$/u, '') !== '/api/public/v1') {
    throw new Error('Public runtime apiBaseUrl must be an absolute HTTP(S) /api/public/v1 URL.')
  }
  return `${url.origin}/api/public/v1`
}

export function configurePublicRuntime(value: PublicRuntimeConfig): void {
  activeConfig = {
    apiBaseUrl: normalizePublicApiBaseUrl(value.apiBaseUrl),
    publicOrigin: normalizePublicOrigin(value.publicOrigin),
  }
}

export function publicApiBaseUrl(): string {
  return activeConfig.apiBaseUrl
}

export function publicApiOrigin(): string {
  return new URL(activeConfig.apiBaseUrl).origin
}

export function runtimePublicOrigin(): string {
  return activeConfig.publicOrigin
}
