import type { ContentEntry } from '@airtek/contracts'
import { asRichTextNode, safeLinkUrl } from '@/lib/richText'
import type { PublicPageModel } from '@/types/content'

const controlCharacters = /[\u0000-\u001f\u007f]/u
const whitespace = /\s+/gu
const scanStatuses = new Set(['clean', 'pending', 'scanning', 'quarantined', 'blocked', 'failed', 'missing'])
const accessStatuses = new Set(['public', 'private', 'restricted'])

export type DownloadScanStatus = 'clean' | 'pending' | 'scanning' | 'quarantined' | 'blocked' | 'failed' | 'missing'
export type DownloadAccessStatus = 'public' | 'private' | 'restricted'

export interface DownloadFileStatus {
  scan?: DownloadScanStatus
  access?: DownloadAccessStatus
}

export interface PublishedDownloadMetadata {
  downloadId: string
  version?: string
  applicableModels: string[]
  resourceType?: string
  fileDescription?: string
  downloadUrl?: string
  downloadUrlState: 'missing' | 'invalid' | 'safe'
  fileStatus?: DownloadFileStatus
}

export type PublishedDownloadListMetadata = Pick<
  PublishedDownloadMetadata,
  'version' | 'applicableModels' | 'resourceType' | 'fileDescription'
>

export interface DownloadAvailability {
  metadata?: PublishedDownloadMetadata
  available: boolean
  href?: string
  reason?: string
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function safeText(value: unknown, maximumLength: number): string | undefined {
  if (typeof value !== 'string' || controlCharacters.test(value)) return undefined
  const normalized = value.normalize('NFKC').replace(whitespace, ' ').trim()
  return normalized && normalized.length <= maximumLength ? normalized : undefined
}

function applicableModels(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > 100) return []
  const normalized = value.map((item) => safeText(item, 120))
  if (normalized.some((item) => !item)) return []
  return [...new Set(normalized as string[])]
}

function fileStatus(value: unknown): DownloadFileStatus | undefined {
  if (!isRecord(value)) return undefined
  const scan = typeof value.scan === 'string' && scanStatuses.has(value.scan)
    ? value.scan as DownloadScanStatus
    : undefined
  const access = typeof value.access === 'string' && accessStatuses.has(value.access)
    ? value.access as DownloadAccessStatus
    : undefined
  return scan || access ? { ...(scan ? { scan } : {}), ...(access ? { access } : {}) } : undefined
}

/** Allows a same-origin root-relative path or an explicit credential-free HTTPS URL. */
export function safePublicDownloadUrl(value: unknown): string | undefined {
  const safe = safeLinkUrl(value)
  if (!safe || (!safe.startsWith('/') && !safe.startsWith('https://'))) return undefined
  if (safe.startsWith('/')) {
    let decodedPath: string
    try {
      decodedPath = decodeURIComponent(new URL(safe, 'https://public.airtek.invalid').pathname)
    } catch {
      return undefined
    }
    if (/^\/(?:admin|api\/admin|api\/devtools)(?:\/|$)/u.test(decodedPath)) return undefined
  }
  return safe
}

/**
 * Download metadata is an optional published-document contract. `fileStatus`
 * is an object shaped as `{ scan, access }`; only `clean/public` is eligible
 * for a public button, and the eligibility check remains separate below.
 */
export function extractPublishedDownloadMetadata(content: ContentEntry | undefined): PublishedDownloadMetadata | undefined {
  if (!content || content.kind !== 'download' || content.status !== 'published' || content.body.schemaVersion !== 1) return undefined
  const root = asRichTextNode(content.body.doc)
  if (!root || root.type !== 'doc') return undefined
  const attrs = root.attrs ?? {}
  const rawDownloadUrl = attrs.downloadUrl
  const downloadUrl = safePublicDownloadUrl(rawDownloadUrl)
  const version = safeText(attrs.version, 80)
  const resourceType = safeText(attrs.resourceType, 80)
  const fileDescription = safeText(attrs.fileDescription, 500)
  const status = fileStatus(attrs.fileStatus)
  return {
    downloadId: content.id,
    ...(version ? { version } : {}),
    applicableModels: applicableModels(attrs.applicableModels),
    ...(resourceType ? { resourceType } : {}),
    ...(fileDescription ? { fileDescription } : {}),
    ...(downloadUrl ? { downloadUrl } : {}),
    downloadUrlState: rawDownloadUrl == null || rawDownloadUrl === '' ? 'missing' : downloadUrl ? 'safe' : 'invalid',
    ...(status ? { fileStatus: status } : {}),
  }
}

/** List cards receive descriptive fields only; URLs and internal file states stay on the detail record. */
export function publishedDownloadListMetadata(content: ContentEntry | undefined): PublishedDownloadListMetadata | undefined {
  if (!content || content.isPlaceholder || !content.seo.indexable) return undefined
  const metadata = extractPublishedDownloadMetadata(content)
  if (!metadata) return undefined
  return {
    ...(metadata.version ? { version: metadata.version } : {}),
    applicableModels: metadata.applicableModels,
    ...(metadata.resourceType ? { resourceType: metadata.resourceType } : {}),
    ...(metadata.fileDescription ? { fileDescription: metadata.fileDescription } : {}),
  }
}

export function publishedDownloadAvailability(page: PublicPageModel): DownloadAvailability {
  const content = page.publishedContent
  const metadata = extractPublishedDownloadMetadata(content)
  if (!content || content.kind !== 'download' || content.status !== 'published' || !metadata) {
    return { available: false, reason: 'No published controlled-file record is available.' }
  }
  if (page.dataState !== 'published' || content.isPlaceholder) {
    return { metadata, available: false, reason: 'This record is a placeholder and does not expose a file.' }
  }
  if (!page.indexable || !content.seo.indexable) {
    return { metadata, available: false, reason: 'This record is not approved for public access.' }
  }
  if (metadata.fileStatus?.scan !== 'clean') {
    return { metadata, available: false, reason: 'A clean file status has not been published for this resource.' }
  }
  if (metadata.fileStatus.access !== 'public') {
    return { metadata, available: false, reason: 'This controlled file is not published for public access.' }
  }
  if (!metadata.downloadUrl || metadata.downloadUrlState !== 'safe') {
    return { metadata, available: false, reason: 'No safe public download URL is published for this resource.' }
  }
  return { metadata, available: true, href: metadata.downloadUrl }
}
