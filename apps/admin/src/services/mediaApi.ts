import { ApiError } from '@airtek/contracts'
import { adminAbsoluteUrl } from './adminApiTransport'
import { getAdminCsrfToken } from './adminCsrf'
import type { MediaAssetSummary } from './contentApi'

async function adminRequest(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers)
  const csrfToken = getAdminCsrfToken()
  if (csrfToken) headers.set('X-CSRF-Token', csrfToken)
  const response = await fetch(adminAbsoluteUrl(path), {
    ...init,
    headers,
    credentials: 'include',
  })
  if (!response.ok) throw await problemError(response)
  return response
}

async function problemError(response: Response): Promise<ApiError> {
  let detail = `请求失败（HTTP ${response.status}）。`
  let requestId = response.headers.get('x-request-id') ?? 'unavailable'
  try {
    const problem: unknown = await response.json()
    if (problem && typeof problem === 'object') {
      const candidate = problem as { detail?: unknown; requestId?: unknown; title?: unknown }
      if (typeof candidate.detail === 'string' && candidate.detail) detail = candidate.detail
      else if (typeof candidate.title === 'string' && candidate.title) detail = candidate.title
      if (typeof candidate.requestId === 'string' && candidate.requestId) requestId = candidate.requestId
    }
  }
  catch {
    // A non-JSON body keeps the status-derived fallback message.
  }
  return new ApiError({
    type: 'about:blank',
    title: `HTTP ${response.status}`,
    status: response.status,
    detail,
    requestId,
  })
}

/** Stores one raster image in the configured private object store. */
export async function uploadMediaAsset(file: File): Promise<MediaAssetSummary> {
  const form = new FormData()
  form.append('file', file, file.name)
  const response = await adminRequest('/api/admin/v1/media/uploads', {
    method: 'POST',
    body: form,
  })
  return await response.json() as MediaAssetSummary
}

/** Records an explicit human review decision; no machine scan is involved. */
export async function reviewMediaAsset(
  assetId: string,
  status: 'clean' | 'quarantined',
  reason: string,
): Promise<MediaAssetSummary> {
  const normalizedReason = reason.trim()
  if (!normalizedReason) throw new TypeError('人工审核决定必须填写原因。')
  if ([...normalizedReason].length > 500) throw new TypeError('人工审核原因不得超过 500 个字符。')
  const response = await adminRequest(`/api/admin/v1/media/assets/${encodeURIComponent(assetId)}/scan`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ status, reason: normalizedReason }),
  })
  return await response.json() as MediaAssetSummary
}
