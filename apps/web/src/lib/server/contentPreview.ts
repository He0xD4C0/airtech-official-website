import type { ContentPreviewResponse } from '@airtek/contracts'
import { isRecord, publicContentProjectionResponse } from '@/lib/publicApiDecoders'

export interface ContentPreviewLoadOptions {
  baseUrl?: string
  fetchImpl?: typeof fetch
}

export class ContentPreviewLoadError extends Error {
  readonly statusCode: 404 | 410

  constructor(statusCode: 404 | 410) {
    super(statusCode === 410 ? 'Content preview has expired.' : 'Content preview was not found.')
    this.name = 'ContentPreviewLoadError'
    this.statusCode = statusCode
  }
}

const maximumTokenLength = 2_048

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function normalizedBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new ContentPreviewLoadError(404)
  return url.toString().replace(/\/$/u, '')
}

function isTimestamp(value: unknown): value is string {
  return typeof value === 'string' && Number.isFinite(Date.parse(value))
}

function parseContentPreviewResponse(value: unknown): ContentPreviewResponse | undefined {
  if (!isRecord(value) || !isTimestamp(value.previewExpiresAt)) return undefined
  try {
    return {
      content: publicContentProjectionResponse(value.content),
      previewExpiresAt: value.previewExpiresAt,
    }
  } catch {
    return undefined
  }
}

export async function loadContentPreview(
  token: string,
  options: ContentPreviewLoadOptions = {},
): Promise<ContentPreviewResponse> {
  if (token.length < 16 || token.length > maximumTokenLength || !/^v1\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/u.test(token)) {
    throw new ContentPreviewLoadError(404)
  }

  let response: Response
  try {
    response = await (options.fetchImpl ?? fetch)(
      `${normalizedBaseUrl(options.baseUrl ?? internalApiBaseUrl())}/content-preview`,
      {
        cache: 'no-store',
        headers: {
          Accept: 'application/json',
          Authorization: `Bearer ${token}`,
        },
      },
    )
  } catch (error) {
    if (error instanceof ContentPreviewLoadError) throw error
    throw new ContentPreviewLoadError(404)
  }

  if (response.status === 410) throw new ContentPreviewLoadError(410)
  if (!response.ok) throw new ContentPreviewLoadError(404)
  let value: unknown
  try {
    value = await response.json()
  } catch {
    throw new ContentPreviewLoadError(404)
  }
  const preview = parseContentPreviewResponse(value)
  if (!preview) throw new ContentPreviewLoadError(404)
  return preview
}
