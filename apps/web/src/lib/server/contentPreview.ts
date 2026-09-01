import type { ContentEntry, ContentPreviewResponse } from '@airtek/contracts'

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
const contentKinds = new Set([
  'home',
  'solution',
  'technology',
  'article',
  'faq',
  'caseStudy',
  'download',
  'company',
  'legal',
  'navigation',
  'footer',
])
const contentStatuses = new Set(['draft', 'scheduled', 'published', 'archived'])

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function normalizedBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new ContentPreviewLoadError(404)
  return url.toString().replace(/\/$/u, '')
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isNullableString(value: unknown): boolean {
  return value === null || typeof value === 'string'
}

function isNullableRevision(value: unknown): boolean {
  return value === null || (Number.isInteger(value) && Number(value) > 0)
}

function isTimestamp(value: unknown): value is string {
  return typeof value === 'string' && Number.isFinite(Date.parse(value))
}

function isContentEntry(value: unknown): value is ContentEntry {
  return isRecord(value)
    && typeof value.id === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value.id)
    && contentKinds.has(String(value.kind))
    && typeof value.slug === 'string'
    && value.slug.length >= 1
    && value.slug.length <= 180
    && value.locale === 'en'
    && typeof value.title === 'string'
    && value.title.trim().length > 0
    && value.title.length <= 300
    && isNullableString(value.summary)
    && isRecord(value.body)
    && value.body.schemaVersion === 1
    && isRecord(value.body.doc)
    && isRecord(value.seo)
    && isNullableString(value.seo.title)
    && isNullableString(value.seo.description)
    && isNullableString(value.seo.canonicalPath)
    && typeof value.seo.indexable === 'boolean'
    && contentStatuses.has(String(value.status))
    && typeof value.isPlaceholder === 'boolean'
    && Number.isInteger(value.currentRevision)
    && Number(value.currentRevision) > 0
    && isNullableRevision(value.publishedRevision)
    && (value.scheduledFor === null || isTimestamp(value.scheduledFor))
    && isTimestamp(value.updatedAt)
}

function isContentPreviewResponse(value: unknown): value is ContentPreviewResponse {
  return isRecord(value)
    && isContentEntry(value.content)
    && isTimestamp(value.previewExpiresAt)
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
  if (!isContentPreviewResponse(value)) throw new ContentPreviewLoadError(404)
  return value
}
