import type {
  ContentBlock,
  ContentBlockKind,
  LinkTargetReference,
  NavigationItem,
} from '@airtek/contracts'
import { parseOpenApiSchema } from '@airtek/contracts'
import type {
  Breadcrumb,
  CardEntry,
  PublicCallToAction,
  PublicLink,
  PublicNewsMetadata,
  PublicPageSection,
} from '@/types/content'
import {
  linkTargetHref,
  type PublicContentProjection,
  type ResolvedRelationCard,
} from '@/types/projection'
import {
  type ArticlePublicationMetadata,
  extractArticleOutline,
} from '@/lib/articlePresentation'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function safeText(value: unknown, maxLength = 1_000): string | undefined {
  if (typeof value !== 'string' || value.length > maxLength) return undefined
  const result = value.trim()
  return result || undefined
}

const blockKinds = new Set<ContentBlockKind>([
  'hero',
  'body',
  'media',
  'featureGrid',
  'evidence',
  'cta',
  'relationCollection',
  'faqCollection',
  'downloadAsset',
  'contactBlock',
])

export function asProjection(value: unknown): PublicContentProjection | undefined {
  if (!isRecord(value) || value.schemaVersion !== 2) return undefined
  if (typeof value.id !== 'string'
    || typeof value.title !== 'string'
    || (typeof value.slug !== 'string' && value.slug !== null)
    || (typeof value.summary !== 'string' && value.summary !== null)
    || typeof value.isPlaceholder !== 'boolean'
    || !Number.isInteger(value.publishedRevision)
    || Number(value.publishedRevision) <= 0
    || typeof value.updatedAt !== 'string') return undefined
  if (!isRecord(value.composition)
    || !Array.isArray(value.composition.blocks)
    || !value.composition.blocks.every((block) => (
      isRecord(block)
      && typeof block.id === 'string'
      && typeof block.type === 'string'
      && blockKinds.has(block.type as ContentBlockKind)
    ))) return undefined
  if (!isRecord(value.typeFields) || typeof value.typeFields.type !== 'string') return undefined
  if (!isRecord(value.seo)) return undefined
  if (!Array.isArray(value.resolvedRelations)
    || !value.resolvedRelations.every(isRelationCard)
    || !Array.isArray(value.resolvedLinks)
    || !value.resolvedLinks.every((entry) => (
      isRecord(entry) && typeof entry.contentId === 'string' && typeof entry.href === 'string'
    ))
    || !Array.isArray(value.resolvedMedia)
    || !value.resolvedMedia.every(isResolvedMedia)) return undefined
  return parseOpenApiSchema<PublicContentProjection>('PublicContentProjection', value)
}

function isResolvedMedia(value: unknown): boolean {
  return isRecord(value)
    && typeof value.assetId === 'string'
    && typeof value.publicUrl === 'string'
    && typeof value.downloadUrl === 'string'
    && typeof value.originalName === 'string'
    && typeof value.mediaType === 'string'
    && Number.isInteger(value.byteSize)
}

function isRelationCard(value: unknown): value is ResolvedRelationCard {
  return isRecord(value)
    && typeof value.relationId === 'string'
    && typeof value.title === 'string'
    && typeof value.href === 'string'
    && (value.entityType === 'content' || value.entityType === 'product')
    && (value.summary === null || typeof value.summary === 'string')
    && (value.eyebrow === null || typeof value.eyebrow === 'string')
    && Array.isArray(value.tags)
    && value.tags.every((tag) => typeof tag === 'string')
}

export function heroBlock(projection: PublicContentProjection): Extract<ContentBlock, { type: 'hero' }> | undefined {
  return projection.composition.blocks.find(
    (block): block is Extract<ContentBlock, { type: 'hero' }> => block.type === 'hero',
  )
}

export function pageTitle(projection: PublicContentProjection): string {
  return safeText(heroBlock(projection)?.heading, 300) ?? safeText(projection.title, 300) ?? ''
}

export function pageEyebrow(projection: PublicContentProjection): string {
  return safeText(heroBlock(projection)?.eyebrow, 160) ?? ''
}

export function pageDescription(projection: PublicContentProjection): string | undefined {
  return safeText(heroBlock(projection)?.lead, 1_000)
    ?? safeText(projection.seo.description, 1_000)
    ?? safeText(projection.summary, 1_000)
}

export function sectionsFromBlocks(projection: PublicContentProjection): PublicPageSection[] {
  return projection.composition.blocks.flatMap((block): PublicPageSection[] => {
    if (block.type === 'featureGrid') {
      return block.items.slice(0, 50).map((item) => ({
        id: item.id,
        eyebrow: safeText(block.heading, 160),
        title: safeText(item.title, 300),
        description: safeText(item.description, 2_000),
      }))
    }
    if (block.type === 'evidence') {
      return block.items.slice(0, 50).map((item) => ({
        id: item.id,
        eyebrow: safeText(item.label, 160),
        description: safeText(item.statement, 2_000),
      }))
    }
    return []
  })
}

export function primaryCtaFromBlocks(projection: PublicContentProjection): PublicCallToAction | undefined {
  const block = projection.composition.blocks.find((entry) => entry.type === 'cta')
  if (!block || block.type !== 'cta') return undefined
  const href = linkTargetHref(block.action.target, projection.resolvedLinks)
  const title = safeText(block.heading, 300)
  if (!href || !title) return undefined
  return {
    label: safeText(block.action.label, 160) ?? '',
    href,
    title,
    description: safeText(block.body, 1_000) ?? '',
    eyebrow: safeText(block.eyebrow, 160),
  }
}

export function relationshipEntriesFromProjection(projection: PublicContentProjection): CardEntry[] {
  const referenced = new Set(
    projection.composition.blocks.flatMap((block) => (
      block.type === 'relationCollection' ? block.relationIds : []
    )),
  )
  const seen = new Set<string>()
  return projection.resolvedRelations.flatMap((card) => {
    if (!referenced.has(card.relationId) || seen.has(card.href)) return []
    seen.add(card.href)
    return [{
      slug: card.href.split('/').at(-1) ?? 'entry',
      title: card.title,
      summary: card.summary ?? '',
      eyebrow: card.eyebrow ?? undefined,
      href: card.href,
      tags: card.tags.length ? card.tags : undefined,
    }]
  })
}

export function breadcrumbsFromProjection(
  projection: PublicContentProjection,
  canonicalPath: string,
): Breadcrumb[] {
  const title = pageTitle(projection)
  if (projection.kind === 'home' || canonicalPath === '/en') return []
  return [{ label: 'Home', href: '/en' }, { label: title || canonicalPath }]
}

export function newsMetadataFrom(projection: PublicContentProjection): PublicNewsMetadata | undefined {
  if (projection.typeFields.type !== 'news') return undefined
  const fields = projection.typeFields
  return {
    category: safeText(fields.category, 120),
    author: safeText(fields.authorDisplayName, 160),
    coverMediaId: fields.cover?.asset?.assetId ?? undefined,
    publishedAt: fields.publicationAt ?? undefined,
    featured: fields.featured,
  }
}

export function articleMetadataFrom(projection: PublicContentProjection): ArticlePublicationMetadata {
  const fields = projection.typeFields
  if (fields.type !== 'article') return { outline: [] }
  const publishedAt = fields.publicationAt ?? undefined
  const updatedAt = projection.updatedAt || undefined
  return {
    ...(fields.authorDisplayName ? { author: fields.authorDisplayName } : {}),
    ...(publishedAt ? { publishedAt, publishedDate: publishedAt.slice(0, 10) } : {}),
    ...(fields.category ? { category: fields.category } : {}),
    ...(updatedAt ? { updatedAt, updatedDate: updatedAt.slice(0, 10) } : {}),
    outline: extractArticleOutline(projection.body),
  }
}

export function linkFromTarget(
  label: string | null | undefined,
  target: LinkTargetReference | undefined,
  projection: PublicContentProjection,
): PublicLink | undefined {
  const text = safeText(label, 160)
  const href = linkTargetHref(target, projection.resolvedLinks)
  return text && href ? { label: text, href } : undefined
}

export function navigationLinksFrom(projection: PublicContentProjection): PublicLink[] {
  if (projection.typeFields.type !== 'navigation') return []
  const links: PublicLink[] = []
  const visit = (items: NavigationItem[]): void => {
    for (const item of items) {
      const link = linkFromTarget(item.label, item.target ?? undefined, projection)
      if (link) links.push(link)
      visit(item.children)
    }
  }
  visit(projection.typeFields.items)
  return [...new Map(links.map((link) => [link.href, link])).values()]
}

export function footerColumnsFrom(projection: PublicContentProjection): Array<{ title: string; links: PublicLink[] }> {
  if (projection.typeFields.type !== 'footer') return []
  return projection.typeFields.columns.flatMap((column) => {
    const title = safeText(column.title, 160)
    const links = column.links.flatMap((link) => {
      const parsed = linkFromTarget(link.label, link.target ?? undefined, projection)
      return parsed ? [parsed] : []
    })
    return title && links.length ? [{ title, links }] : []
  })
}

export function legalLinksFrom(projection: PublicContentProjection): PublicLink[] {
  if (projection.typeFields.type !== 'footer') return []
  return projection.typeFields.legalLinks.flatMap((link) => {
    const parsed = linkFromTarget(link.label, link.target ?? undefined, projection)
    return parsed ? [parsed] : []
  })
}

export interface GeneralInformationProjection {
  brandName: string
  brandLine?: string
  homePath: string
  footerStatement?: string
  copyrightText?: string
  defaultSeo: { title?: string; description?: string }
  organization: { name: string; url?: string; logoUrl?: string }
  navigationCta?: PublicLink
  socialLinks: Array<{ service: string; url: string }>
}

export function generalInformationFrom(projection: PublicContentProjection): GeneralInformationProjection | undefined {
  if (projection.typeFields.type !== 'generalInformation') return undefined
  const fields = projection.typeFields
  const brandName = safeText(fields.organizationName, 200)
  const homePath = safeText(fields.homePath, 200)
  if (!brandName || !homePath || (homePath !== '/en' && !homePath.startsWith('/en/'))) return undefined
  return {
    brandName,
    brandLine: safeText(fields.brandLine, 300),
    homePath,
    footerStatement: safeText(fields.footerStatement, 1_000),
    copyrightText: safeText(fields.copyrightTemplate, 300),
    defaultSeo: {
      title: safeText(fields.defaultSeo.title, 300),
      description: safeText(fields.defaultSeo.description, 1_000),
    },
    organization: { name: brandName },
    navigationCta: fields.navigationCta
      ? linkFromTarget(fields.navigationCta.label, fields.navigationCta.target, projection)
      : undefined,
    socialLinks: fields.socialLinks.flatMap((link) => {
      const service = safeText(link.service, 80)
      const url = safeText(link.url, 2_000)
      return service && url ? [{ service, url }] : []
    }),
  }
}
