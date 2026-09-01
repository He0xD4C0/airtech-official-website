const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f]/u
const COLLAPSIBLE_WHITESPACE = /\s+/gu

const DOWNLOAD_SCAN_STATUSES = new Set([
  'clean',
  'pending',
  'scanning',
  'quarantined',
  'blocked',
  'failed',
  'missing',
] as const)
const DOWNLOAD_ACCESS_STATUSES = new Set(['public', 'private', 'restricted'] as const)

export type ArticleAuthorType = 'Person' | 'Organization'
export type DownloadScanStatus = 'clean' | 'pending' | 'scanning' | 'quarantined' | 'blocked' | 'failed' | 'missing'
export type DownloadAccessStatus = 'public' | 'private' | 'restricted'

export interface ArticleDocumentAttrs extends Record<string, unknown> {
  author?: string
  authorType?: ArticleAuthorType
  publishedAt?: string
  category?: string
}

export interface DownloadDocumentAttrs extends Record<string, unknown> {
  version?: string
  applicableModels?: string[]
  resourceType?: string
  fileDescription?: string
  downloadUrl?: string
  fileStatus?: {
    scan?: DownloadScanStatus
    access?: DownloadAccessStatus
  }
}

export type SafeDocumentAttrs = Record<string, unknown>

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function safeText(value: unknown, maximumLength: number): string | undefined {
  if (typeof value !== 'string' || CONTROL_CHARACTERS.test(value)) return undefined
  const normalized = value.normalize('NFKC').replace(COLLAPSIBLE_WHITESPACE, ' ').trim()
  return normalized && normalized.length <= maximumLength ? normalized : undefined
}

function safePublishedAt(value: unknown): string | undefined {
  const text = safeText(value, 40)
  if (!text || !/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$/u.test(text)) return undefined
  const [year, month, day] = text.slice(0, 10).split('-').map(Number)
  const date = new Date(Date.UTC(year, month - 1, day))
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return undefined
  return Number.isFinite(new Date(text).valueOf()) ? text : undefined
}

function safeApplicableModels(value: unknown): string[] | undefined {
  if (!Array.isArray(value) || value.length > 100) return undefined
  const normalized = value.map((item) => safeText(item, 120))
  if (normalized.some((item) => !item)) return undefined
  const models = [...new Set(normalized as string[])]
  return models.length ? models : undefined
}

/** Matches the public download contract: same-origin root paths or credential-free HTTPS. */
export function safePublicDownloadUrl(value: unknown): string | undefined {
  if (typeof value !== 'string' || value.length > 2_048 || CONTROL_CHARACTERS.test(value)) return undefined
  const trimmed = value.trim()
  if (trimmed.startsWith('/')) {
    if (trimmed.startsWith('//') || trimmed.includes('\\')) return undefined
    try {
      const parsed = new URL(trimmed, 'https://public.airtek.invalid')
      if (parsed.origin !== 'https://public.airtek.invalid') return undefined
      const decodedPath = decodeURIComponent(parsed.pathname)
      if (/^\/(?:admin|api\/admin|api\/devtools)(?:\/|$)/u.test(decodedPath)) return undefined
      return `${parsed.pathname}${parsed.search}${parsed.hash}`
    } catch {
      return undefined
    }
  }
  try {
    const parsed = new URL(trimmed)
    if (parsed.protocol !== 'https:' || parsed.username || parsed.password) return undefined
    return parsed.toString()
  } catch {
    return undefined
  }
}

function articleAttrs(attrs: Record<string, unknown>): ArticleDocumentAttrs {
  const author = safeText(attrs.author, 160)
  const authorType = attrs.authorType === 'Person' || attrs.authorType === 'Organization'
    ? attrs.authorType
    : undefined
  const publishedAt = safePublishedAt(attrs.publishedAt)
  const category = safeText(attrs.category, 120)
  return {
    ...(author ? { author } : {}),
    ...(author && authorType ? { authorType } : {}),
    ...(publishedAt ? { publishedAt } : {}),
    ...(category ? { category } : {}),
  }
}

function downloadAttrs(attrs: Record<string, unknown>): DownloadDocumentAttrs {
  const version = safeText(attrs.version, 80)
  const applicableModels = safeApplicableModels(attrs.applicableModels)
  const resourceType = safeText(attrs.resourceType, 80)
  const fileDescription = safeText(attrs.fileDescription, 500)
  const downloadUrl = safePublicDownloadUrl(attrs.downloadUrl)
  const rawFileStatus = isRecord(attrs.fileStatus) ? attrs.fileStatus : {}
  const scan = typeof rawFileStatus.scan === 'string' && DOWNLOAD_SCAN_STATUSES.has(rawFileStatus.scan as DownloadScanStatus)
    ? rawFileStatus.scan as DownloadScanStatus
    : undefined
  const access = typeof rawFileStatus.access === 'string' && DOWNLOAD_ACCESS_STATUSES.has(rawFileStatus.access as DownloadAccessStatus)
    ? rawFileStatus.access as DownloadAccessStatus
    : undefined
  return {
    ...(version ? { version } : {}),
    ...(applicableModels ? { applicableModels } : {}),
    ...(resourceType ? { resourceType } : {}),
    ...(fileDescription ? { fileDescription } : {}),
    ...(downloadUrl ? { downloadUrl } : {}),
    ...(scan || access ? { fileStatus: { ...(scan ? { scan } : {}), ...(access ? { access } : {}) } } : {}),
  }
}

/**
 * Root document attributes are a versioned published-data contract, not a free-form HTML surface.
 * FAQ questions remain in the structured body, so FAQ currently has no root-attribute allowlist.
 */
export function sanitizeContentDocumentAttrs(kind: string, value: unknown): SafeDocumentAttrs {
  if (!isRecord(value)) return {}
  if (kind === 'article') return articleAttrs(value)
  if (kind === 'download') return downloadAttrs(value)
  return {}
}
