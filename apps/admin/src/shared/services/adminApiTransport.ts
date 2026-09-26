import { createContractClient } from '@airtek/contracts'
import { adminApiBaseUrl } from '@/app/runtimeConfig'
import { captureAdminCsrfToken, getAdminCsrfToken } from '@/shared/services/adminCsrf'
import type { CursorPageRequest } from '@/shared/services/cursorPagination'

const baseUrl = adminApiBaseUrl()

export function randomRequestId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

export const adminContractClient = createContractClient({
  baseUrl: new URL(baseUrl).origin,
  onResponse: captureAdminCsrfToken,
  getCsrfToken: getAdminCsrfToken,
})

export function cursorQuery(pagination: CursorPageRequest = {}): { cursor?: string; limit?: number } {
  if (
    pagination.limit !== undefined
    && (!Number.isInteger(pagination.limit) || pagination.limit < 1 || pagination.limit > 100)
  ) {
    throw new RangeError('分页大小必须是 1 到 100 之间的整数。')
  }
  return {
    ...(pagination.cursor ? { cursor: pagination.cursor } : {}),
    ...(pagination.limit === undefined ? {} : { limit: pagination.limit }),
  }
}

export function revisionEtag(revision: number | undefined, allowZero = false): string {
  if (!Number.isInteger(revision) || (revision ?? -1) < (allowZero ? 0 : 1)) {
    throw new TypeError('A positive entity revision is required for this update.')
  }
  return `"revision-${revision}"`
}

export function draftEtag(version: number | undefined, allowZero = false): string {
  const value = version ?? -1
  if (!Number.isInteger(value) || value < (allowZero ? 0 : 1)) {
    throw new TypeError('A valid draft version is required for this update.')
  }
  return `"draft-${value}"`
}

export function adminAbsoluteUrl(path: string): string {
  const base = new URL(baseUrl)
  const url = new URL(path, base.origin)
  if (url.origin !== base.origin || !url.pathname.startsWith('/api/admin/v1/')) {
    throw new Error('The operation stream URL is outside the configured Admin API boundary.')
  }
  return url.toString()
}
