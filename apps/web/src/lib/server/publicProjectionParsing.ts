import type { ContentEntry } from '@airtek/contracts'
import type { createPublicApiClient } from '@/lib/publicApiClient'
import type { SiteBootstrapResponse } from '@/lib/publicApiTypes'
import { safeImageUrl, safeLinkUrl } from '@/lib/richText'
import type {
  Breadcrumb,
  CardEntry,
  ProductFamilyProjection,
  PublicCallToAction,
  PublicLink,
  PublicPageSection,
  PublicSiteBootstrap,
} from '@/types/content'
import { PublicPageDataError } from './publicPageDataTypes'

const unsafeControls = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u
export const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function safeText(value: unknown, maxLength = 1_000): string | undefined {
  if (typeof value !== 'string' || value.length > maxLength || unsafeControls.test(value)) return undefined
  const result = value.trim()
  return result || undefined
}

function attrsOf(content: ContentEntry | null | undefined): Record<string, unknown> {
  const doc = content?.body.doc
  return isRecord(doc) && isRecord(doc.attrs) ? doc.attrs : {}
}

function internalPublicLink(value: unknown): PublicLink | undefined {
  if (!isRecord(value)) return undefined
  const label = safeText(value.label, 160)
  const href = safeLinkUrl(value.href)
  if (!label || !href) return undefined
  if (href.startsWith('/') && href !== '/en' && !href.startsWith('/en/')) return undefined
  return { label, href }
}

function links(value: unknown): PublicLink[] {
  if (!Array.isArray(value)) return []
  return value.slice(0, 100).flatMap((item) => {
    const link = internalPublicLink(item)
    return link ? [link] : []
  })
}

export function parseSiteBootstrap(value: SiteBootstrapResponse): PublicSiteBootstrap {
  const information = value.generalInformation
  if (!information
    || information.status !== 'published'
    || !information.publishedRevision
    || !value.navigation
    || !value.navigation.publishedRevision
    || !value.footer
    || !value.footer.publishedRevision) {
    throw new PublicPageDataError('The public site bootstrap has no complete published projection.', 503)
  }

  const payload = information.payload
  const brandName = safeText(payload.brandName, 160)
  const homePath = safeLinkUrl(payload.homePath)
  const organization = isRecord(payload.organization) ? payload.organization : undefined
  const organizationName = safeText(organization?.name, 200)
  const defaultSeo = isRecord(payload.defaultSeo) ? payload.defaultSeo : undefined
  if (!brandName || !homePath || !homePath.startsWith('/en') || !organizationName) {
    throw new PublicPageDataError('The published General Information projection is incomplete.', 503)
  }

  const navigationAttrs = attrsOf(value.navigation)
  const footerAttrs = attrsOf(value.footer)
  const footerColumns = Array.isArray(footerAttrs.columns)
    ? footerAttrs.columns.slice(0, 20).flatMap((column) => {
        if (!isRecord(column)) return []
        const title = safeText(column.title, 160)
        const columnLinks = links(column.links)
        return title && columnLinks.length ? [{ title, links: columnLinks }] : []
      })
    : []

  return {
    brandName,
    brandLine: safeText(payload.brandLine, 300),
    homePath,
    footerStatement: safeText(payload.footerStatement, 1_000),
    copyrightText: safeText(payload.copyrightText, 300),
    defaultSeo: {
      title: safeText(defaultSeo?.title, 300),
      description: safeText(defaultSeo?.description, 1_000),
    },
    organization: {
      name: organizationName,
      url: safeLinkUrl(organization?.url),
      logoUrl: safeImageUrl(organization?.logoUrl),
    },
    navigation: links(navigationAttrs.items),
    navigationCta: internalPublicLink(payload.navigationCta),
    footerColumns,
    legalLinks: links(footerAttrs.legalLinks),
    productFamilies: value.productFamilies
      .map((family): ProductFamilyProjection => ({ ...family }))
      .sort((left, right) => left.sortOrder - right.sortOrder),
    motorTechnologies: value.motorTechnologies,
    generatedAt: value.generatedAt,
    publishedRevision: information.publishedRevision,
    isPlaceholder: information.isPlaceholder || value.navigation.isPlaceholder || value.footer.isPlaceholder,
  }
}

export function pageSlots(content: ContentEntry | null): Record<string, unknown> {
  const attrs = attrsOf(content)
  return isRecord(attrs.pageSlots) ? attrs.pageSlots : {}
}

function section(value: unknown, index: number): PublicPageSection | undefined {
  if (!isRecord(value)) return undefined
  const id = safeText(value.id, 120) ?? `section-${index + 1}`
  if (!/^[A-Za-z][A-Za-z0-9_-]{0,119}$/u.test(id)) return undefined
  const result: PublicPageSection = {
    id,
    eyebrow: safeText(value.eyebrow, 160),
    title: safeText(value.title, 300),
    description: safeText(value.description, 2_000),
    links: links(value.links),
  }
  return result.eyebrow || result.title || result.description || result.links?.length ? result : undefined
}

export function sectionsFrom(content: ContentEntry | null): PublicPageSection[] {
  const value = pageSlots(content).sections
  if (!Array.isArray(value)) return []
  return value.slice(0, 100).flatMap((item, index) => {
    const parsed = section(item, index)
    return parsed ? [parsed] : []
  })
}

export function primaryCtaFrom(content: ContentEntry | null): PublicCallToAction | undefined {
  const value = pageSlots(content).primaryCta
  if (!isRecord(value)) return undefined
  const link = internalPublicLink(value)
  const title = safeText(value.title, 300)
  const description = safeText(value.description, 1_000)
  if (!link || !title || !description) return undefined
  return { ...link, title, description, eyebrow: safeText(value.eyebrow, 160) }
}

function relationshipValues(content: ContentEntry | null): unknown[] {
  const value = pageSlots(content).relationships
  if (Array.isArray(value)) return value
  if (!isRecord(value)) return []
  return Object.values(value).flatMap((item) => Array.isArray(item) ? item : [])
}

export function relationshipEntries(content: ContentEntry | null): CardEntry[] {
  const seen = new Set<string>()
  return relationshipValues(content).slice(0, 200).flatMap((value) => {
    if (!isRecord(value) || value.entityType === 'product') return []
    const href = safeLinkUrl(value.href)
    const title = safeText(value.title, 300)
    if (!href || (href !== '/en' && !href.startsWith('/en/')) || !title || seen.has(href)) return []
    seen.add(href)
    return [{
      slug: safeText(value.slug, 180) ?? href.split('/').at(-1) ?? 'entry',
      title,
      summary: safeText(value.summary, 1_000) ?? '',
      eyebrow: safeText(value.eyebrow, 160),
      href,
      tags: Array.isArray(value.tags)
        ? value.tags.flatMap((tag) => {
            const text = safeText(tag, 120)
            return text ? [text] : []
          }).slice(0, 20)
        : undefined,
    }]
  })
}

export async function verifiedProductRelationshipEntries(
  content: ContentEntry | null,
  client: ReturnType<typeof createPublicApiClient>,
  families: ProductFamilyProjection[],
): Promise<CardEntry[]> {
  const candidates = relationshipValues(content).flatMap((value) => {
    if (!isRecord(value) || value.entityType !== 'product') return []
    const href = safeLinkUrl(value.href)
    const match = href?.match(/^\/en\/products\/([^/]+)\/([^/]+)$/u)
    if (!match) return []
    const family = families.find((item) => item.slug === match[1])
    const slug = safeText(value.slug, 180) ?? match[2]
    return family && slug === match[2] && slugPattern.test(slug) ? [{ family, slug }] : []
  })
  const unique = [...new Map(candidates.map((candidate) => [
    `${candidate.family.code}:${candidate.slug}`,
    candidate,
  ])).values()].slice(0, 24)
  const resolved = await Promise.all(unique.map(async ({ family, slug }): Promise<CardEntry | undefined> => {
    try {
      const product = await client.getProduct(slug, family.code)
      if (product.family !== family.code || product.slug !== slug || !product.publishedRevision) return undefined
      return {
        slug: product.slug,
        title: product.title,
        summary: product.summary?.trim() ?? '',
        eyebrow: family.name,
        href: `/en/products/${family.slug}/${product.slug}`,
      }
    } catch {
      return undefined
    }
  }))
  return resolved.flatMap((entry) => entry ? [entry] : [])
}

export function breadcrumbsFrom(content: ContentEntry | null, currentTitle: string): Breadcrumb[] {
  const value = pageSlots(content).breadcrumbs
  const result = Array.isArray(value)
    ? value.slice(0, 20).flatMap((item) => {
        if (!isRecord(item)) return []
        const label = safeText(item.label, 160)
        if (!label) return []
        const href = item.href === undefined ? undefined : safeLinkUrl(item.href)
        if (href && href.startsWith('/') && href !== '/en' && !href.startsWith('/en/')) return []
        return [{ label, href }]
      })
    : []
  return result.length ? result : [{ label: currentTitle }]
}
