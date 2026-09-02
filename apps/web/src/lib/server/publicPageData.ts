import type { ContentEntry, Product, ProductContext, ProductFamily } from '@airtek/contracts'
import {
  createPublicApiClient,
  PublicApiError,
  type NewsEntryResponse,
  type PublicDiscoveryEntryResponse,
  type RouteProjectionResponse,
  type SiteBootstrapResponse,
} from '@/lib/api'
import { isSearchablePublicPath } from '@/lib/publicPaths'
import { safeImageUrl, safeLinkUrl } from '@/lib/richText'
import type {
  Breadcrumb,
  CardEntry,
  PageKind,
  ProductFamilyProjection,
  PublicCallToAction,
  PublicLink,
  PublicPageModel,
  PublicPageSection,
  PublicSiteBootstrap,
  RfqType,
} from '@/types/content'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/lib/productPagination'

export interface PublicPageDataOptions {
  baseUrl?: string
  fetchImpl?: typeof fetch
  productSlug?: string
  productFamily?: ProductFamily
}

export interface LoadedPublicPageData {
  page: PublicPageModel
  site: PublicSiteBootstrap
}

export class PublicPageDataError extends Error {
  constructor(message: string, readonly status: 404 | 503) {
    super(message)
    this.name = 'PublicPageDataError'
  }
}

const unsafeControls = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u
const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u

function internalApiBaseUrl(): string {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function normalizedBaseUrl(value: string): string {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new PublicPageDataError('The public API origin is invalid.', 503)
  return url.toString().replace(/\/$/u, '')
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function safeText(value: unknown, maxLength = 1_000): string | undefined {
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

function parseSiteBootstrap(value: SiteBootstrapResponse): PublicSiteBootstrap {
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

function pageSlots(content: ContentEntry | null): Record<string, unknown> {
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

function sectionsFrom(content: ContentEntry | null): PublicPageSection[] {
  const value = pageSlots(content).sections
  if (!Array.isArray(value)) return []
  return value.slice(0, 100).flatMap((item, index) => {
    const parsed = section(item, index)
    return parsed ? [parsed] : []
  })
}

function primaryCtaFrom(content: ContentEntry | null): PublicCallToAction | undefined {
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

function relationshipEntries(content: ContentEntry | null): CardEntry[] {
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

async function verifiedProductRelationshipEntries(
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

function breadcrumbsFrom(content: ContentEntry | null, currentTitle: string): Breadcrumb[] {
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

interface TemplateResolution {
  kind: PageKind
  collection?: PublicPageModel['collection']
}

function templateResolution(route: RouteProjectionResponse): TemplateResolution {
  const key = route.templateKey.toLowerCase()
  const compact = key.replaceAll('-', '')
  const entity = route.entityType.toLowerCase()
  const indexLike = /(?:index|list|collection|landing)$/u.test(compact)
  if (route.path === '/en/products') return { kind: 'catalog' }
  if (route.path === '/en/products/selector') return { kind: 'selector' }
  if (route.path === '/en/products/compare') return { kind: 'compare' }
  if (/^\/en\/products\/[^/]+$/u.test(route.path)) return { kind: 'catalog' }
  if (entity === 'product' || /^\/en\/products\/[^/]+\/[^/]+$/u.test(route.path)) return { kind: 'product-detail' }
  if (route.path === '/en/request-a-quote') return { kind: 'rfq-router' }
  if (/^\/en\/request-a-quote\/(?:product|selection|project|replacement)$/u.test(route.path)) return { kind: 'rfq-form' }
  if (route.path === '/en/search') return { kind: 'search' }
  if (route.path === '/en/company/about') return { kind: 'about' }
  if (route.path === '/en/company/contact') return { kind: 'contact' }
  if (route.path === '/en/resources/news') return { kind: 'news' }
  if (/^\/en\/resources\/news\/[^/]+$/u.test(route.path)) return { kind: 'news-detail' }
  if (route.path === '/en/resources/downloads') return { kind: 'downloads', collection: 'downloads' }
  if (/^\/en\/resources\/downloads\/[^/]+$/u.test(route.path)) return { kind: 'detail', collection: 'downloads' }
  if (key === 'home') return { kind: 'home' }
  if (compact.includes('productdetail')) return { kind: 'product-detail' }
  if (compact.includes('productfamily') || key === 'products' || compact.includes('productindex') || key === 'catalog') return { kind: 'catalog' }
  if (key.includes('selector')) return { kind: 'selector' }
  if (key.includes('compare')) return { kind: 'compare' }
  if (key.includes('news')) return { kind: indexLike || route.path === '/en/resources/news' ? 'news' : 'news-detail' }
  if (key.includes('faq')) return { kind: 'faq' }
  if (key.includes('download')) return { kind: 'downloads', collection: 'downloads' }
  if (key.includes('about')) return { kind: 'about' }
  if (key.includes('contact')) return { kind: 'contact' }
  if (key.includes('search')) return { kind: 'search' }
  if (key.includes('legal') || entity === 'legal') return { kind: 'legal' }
  if (key.includes('rfq')) return { kind: key.includes('form') ? 'rfq-form' : 'rfq-router' }

  for (const [token, collection] of [
    ['solution', 'solutions'],
    ['technology', 'technology'],
    ['article', 'articles'],
    ['case', 'cases'],
  ] as const) {
    if (key.includes(token) || entity.includes(token)) {
      const basePath = collection === 'solutions'
        ? '/en/solutions'
        : collection === 'technology'
          ? '/en/technology'
          : collection === 'articles'
            ? '/en/resources/articles'
            : '/en/resources/case-studies'
      return { kind: indexLike || route.path === basePath ? 'collection' : 'detail', collection }
    }
  }
  throw new PublicPageDataError(`The published route template "${route.templateKey}" is unsupported.`, 503)
}

function rfqType(path: string): RfqType | undefined {
  const match = path.match(/^\/en\/request-a-quote\/(product|selection|project|replacement)$/u)
  return match?.[1] as RfqType | undefined
}

function pageFromRoute(route: RouteProjectionResponse, site: PublicSiteBootstrap): PublicPageModel {
  const resolved = templateResolution(route)
  const content = route.page
  const slots = pageSlots(content)
  const hero = isRecord(slots.hero) ? slots.hero : {}
  const title = safeText(hero.title, 300) ?? safeText(content?.title, 300)
  const description = safeText(hero.description, 1_000)
    ?? safeText(content?.seo.description, 1_000)
    ?? safeText(content?.summary, 1_000)
    ?? site.defaultSeo.description
  const externallyTitled = resolved.kind === 'product-detail' || resolved.kind === 'news-detail'
  if (!title && !externallyTitled) {
    throw new PublicPageDataError('The published page projection is missing its required title.', 503)
  }
  if (!content && resolved.kind !== 'product-detail' && resolved.kind !== 'news-detail') {
    throw new PublicPageDataError('The published route has no page projection.', 503)
  }
  const contentRevision = content?.publishedRevision
  const publishedRevision = route.publishedRevision ?? contentRevision
  if (!publishedRevision) throw new PublicPageDataError('The published route has no revision.', 503)
  const placeholder = content?.isPlaceholder === true
  const analyticsEntityId = route.entityId ?? content?.id
  const policyNoIndex = resolved.kind === 'compare'
    || resolved.kind === 'search'
    || resolved.kind === 'rfq-form'

  return {
    kind: resolved.kind,
    canonicalPath: route.path,
    title: title ?? '',
    metaTitle: safeText(content?.seo.title, 300) ?? title ?? site.defaultSeo.title ?? '',
    description: description ?? '',
    eyebrow: safeText(hero.eyebrow, 160) ?? safeText(slots.eyebrow, 160) ?? '',
    breadcrumbs: breadcrumbsFrom(content, title ?? ''),
    indexable: !policyNoIndex
      && route.indexable
      && route.dataClass !== 'developmentFixture'
      && !site.isPlaceholder
      && !placeholder
      && content?.seo.indexable !== false,
    collection: resolved.collection,
    slug: content?.slug === 'index' ? undefined : content?.slug,
    rfqType: resolved.kind === 'rfq-form' ? rfqType(route.path) : undefined,
    entries: relationshipEntries(content),
    dataState: placeholder ? 'placeholder' : 'published',
    placeholderReason: placeholder ? 'This published record is explicitly marked as placeholder content.' : undefined,
    publishedContent: content ?? undefined,
    primaryCta: primaryCtaFrom(content),
    sections: sectionsFrom(content),
    productFamilies: site.productFamilies,
    motorTechnologies: site.motorTechnologies,
    analyticsContext: analyticsEntityId ? {
      contentKind: route.entityType === 'product' ? 'product' : 'content',
      contentId: analyticsEntityId,
      publishedRevision,
    } : undefined,
    dataClass: route.dataClass,
  }
}

function validDetailPath(path: string, prefix: string): boolean {
  if (!path.startsWith(`${prefix}/`)) return false
  const slug = path.slice(prefix.length + 1)
  return slugPattern.test(slug) && slug !== 'index'
}

function discoveryEntryType(entry: PublicDiscoveryEntryResponse): string {
  if (entry.entityType === 'product') return 'Product'
  if (entry.path.startsWith('/en/solutions/')) return 'Solution'
  if (entry.path.startsWith('/en/technology/')) return 'Technology'
  if (entry.path.startsWith('/en/resources/articles/')) return 'Article'
  if (entry.path.startsWith('/en/resources/news/')) return 'News'
  if (entry.path.startsWith('/en/resources/faqs/')) return 'FAQ'
  if (entry.path.startsWith('/en/resources/case-studies/')) return 'Case study'
  if (entry.path.startsWith('/en/resources/downloads/')) return 'Download'
  if (entry.path.startsWith('/en/company/')) return 'Company'
  return 'Page'
}

function discoveryCards(entries: PublicDiscoveryEntryResponse[], page: PublicPageModel): CardEntry[] {
  const prefixes: Partial<Record<NonNullable<PublicPageModel['collection']>, string>> = {
    solutions: '/en/solutions',
    technology: '/en/technology',
    articles: '/en/resources/articles',
    cases: '/en/resources/case-studies',
    downloads: '/en/resources/downloads',
  }
  const prefix = page.collection ? prefixes[page.collection] : undefined
  const seen = new Set<string>()
  return entries.flatMap((entry) => {
    const title = safeText(entry.title, 300)
    const allowed = page.kind === 'search'
      ? isSearchablePublicPath(entry.entityType, entry.path)
      : Boolean(prefix && entry.entityType === 'content' && validDetailPath(entry.path, prefix))
    if (!allowed || !title || seen.has(entry.path)) return []
    seen.add(entry.path)
    return [{
      slug: entry.path.split('/').at(-1) ?? 'entry',
      title,
      summary: safeText(entry.summary, 1_000) ?? '',
      eyebrow: discoveryEntryType(entry),
      href: entry.path,
    }]
  })
}

function productFamilyCode(page: PublicPageModel, path: string): ProductFamily | undefined {
  const match = path.match(/^\/en\/products\/([^/]+)$/u)
  if (!match || match[1] === 'selector' || match[1] === 'compare') return undefined
  const family = page.productFamilies?.find((item) => item.slug === match[1])
  if (!family) throw new PublicPageDataError('The published product family is unavailable.', 404)
  page.category = family.slug
  return family.code
}

function productContext(product: Product): ProductContext | undefined {
  if (!product.model?.trim() || !product.publishedRevision) return undefined
  return {
    productId: product.id,
    stableId: product.stableId,
    model: product.model,
    publishedRevision: product.publishedRevision,
  }
}

function newsCard(entry: NewsEntryResponse): CardEntry | undefined {
  const developmentFixture = entry.dataClass === 'developmentFixture'
  if ((entry.content.isPlaceholder && !developmentFixture) || !entry.content.publishedRevision) return undefined
  const slug = entry.content.slug
  if (!slugPattern.test(slug)) return undefined
  return {
    slug,
    title: entry.content.title,
    summary: entry.content.summary ?? '',
    eyebrow: entry.category ?? undefined,
    href: `/en/resources/news/${slug}`,
    category: entry.category ?? undefined,
    author: entry.authorDisplayName ?? undefined,
    publishedAt: entry.publishedAt ?? undefined,
    featured: entry.featured,
  }
}

async function enrichPage(
  page: PublicPageModel,
  route: RouteProjectionResponse,
  client: ReturnType<typeof createPublicApiClient>,
  options: PublicPageDataOptions,
): Promise<PublicPageModel> {
  try {
    if (page.kind === 'catalog') {
      const family = productFamilyCode(page, route.path)
      const products = await client.listProducts({ limit: PUBLIC_PRODUCT_PAGE_SIZE, family })
      page.publishedProducts = products.items.filter((product) => !family || product.family === family)
      page.productNextCursor = products.nextCursor
    } else if (page.kind === 'product-detail') {
      const match = route.path.match(/^\/en\/products\/([^/]+)\/([^/]+)$/u)
      if (!match || !slugPattern.test(match[2])) throw new PublicPageDataError('The product route is invalid.', 404)
      const family = page.productFamilies?.find((item) => item.slug === match[1])
      if (!family) throw new PublicPageDataError('The published product family is unavailable.', 404)
      const product = await client.getProduct(match[2], family.code)
      if (product.family !== family.code || product.slug !== match[2]) throw new PublicPageDataError('The published product route does not match the product record.', 404)
      if (!product.publishedRevision) throw new PublicPageDataError('The published product record has no revision.', 503)
      page.category = family.slug
      page.slug = product.slug
      page.title = product.title
      page.metaTitle = product.seo.title?.trim() || route.page?.seo.title?.trim() || product.title
      page.description = product.seo.description?.trim() || product.summary?.trim() || page.description
      page.eyebrow = product.model?.trim() || product.stableId
      page.indexable = page.indexable && product.indexable
      page.publishedProduct = product
      page.productContext = productContext(product)
      page.breadcrumbs = [...page.breadcrumbs.slice(0, -1), { label: product.title }]
      page.analyticsContext = {
        contentKind: 'product',
        contentId: product.id,
        publishedRevision: product.publishedRevision,
      }
    } else if (page.kind === 'news') {
      const news = await client.listNews({ locale: 'en', limit: 48 })
      page.entries = news.items.flatMap((entry) => {
        const card = newsCard(entry)
        return card ? [card] : []
      })
      page.newsNextCursor = news.nextCursor
    } else if (page.kind === 'news-detail') {
      const slug = route.path.split('/').at(-1)
      if (!slug || !slugPattern.test(slug)) throw new PublicPageDataError('The News route is invalid.', 404)
      const news = await client.getNews(slug)
      if (news.content.slug !== slug) throw new PublicPageDataError('The published News route does not match its record.', 404)
      if (!news.content.publishedRevision) throw new PublicPageDataError('The published News record has no revision.', 503)
      page.publishedContent = news.content
      page.slug = slug
      page.title = news.content.title
      page.metaTitle = news.content.seo.title?.trim() || news.content.title
      page.description = news.content.seo.description?.trim() || news.content.summary?.trim() || page.description
      const newsSlots = pageSlots(news.content)
      const newsHero = isRecord(newsSlots.hero) ? newsSlots.hero : {}
      page.eyebrow = safeText(newsHero.eyebrow, 160) ?? news.category ?? page.eyebrow
      page.breadcrumbs = breadcrumbsFrom(news.content, news.content.title)
      page.primaryCta = primaryCtaFrom(news.content)
      page.sections = sectionsFrom(news.content)
      page.entries = relationshipEntries(news.content)
      page.indexable = page.indexable
        && !news.content.isPlaceholder
        && news.dataClass !== 'developmentFixture'
        && news.content.seo.indexable
      page.dataClass = news.dataClass
      page.dataState = news.content.isPlaceholder ? 'placeholder' : 'published'
      page.placeholderReason = news.content.isPlaceholder
        ? 'This published record is explicitly marked as placeholder content.'
        : undefined
      page.newsMetadata = {
        category: news.category ?? undefined,
        author: news.authorDisplayName ?? undefined,
        coverMediaId: news.coverMediaId ?? undefined,
        publishedAt: news.publishedAt ?? undefined,
        featured: news.featured,
      }
      page.analyticsContext = {
        contentKind: 'news',
        contentId: news.content.id,
        publishedRevision: news.content.publishedRevision,
      }
    } else if (page.kind === 'collection' || page.kind === 'downloads' || page.kind === 'search') {
      const discovery = await client.getDiscovery()
      page.entries = discoveryCards(discovery.entries, page)
    } else if (page.kind === 'rfq-form' && page.rfqType === 'product' && options.productSlug && slugPattern.test(options.productSlug)) {
      const product = await client.getProduct(options.productSlug, options.productFamily)
      page.productContext = productContext(product)
    }
    const relatedProducts = await verifiedProductRelationshipEntries(route.page, client, page.productFamilies ?? [])
    if (relatedProducts.length) {
      const existing = new Set((page.entries ?? []).map((entry) => entry.href))
      page.entries = [...(page.entries ?? []), ...relatedProducts.filter((entry) => !existing.has(entry.href))]
    }
    return page
  } catch (cause) {
    if (cause instanceof PublicPageDataError) throw cause
    if (cause instanceof PublicApiError && cause.status === 404) throw new PublicPageDataError('The requested published record does not exist.', 404)
    throw new PublicPageDataError('A required public projection is temporarily unavailable.', 503)
  }
}

export async function loadPublicPageData(
  path: string,
  options: PublicPageDataOptions = {},
): Promise<LoadedPublicPageData> {
  const baseUrl = normalizedBaseUrl(options.baseUrl ?? internalApiBaseUrl())
  const client = createPublicApiClient({ baseUrl, fetchImpl: options.fetchImpl })
  const [bootstrapResult, routeResult] = await Promise.allSettled([
    client.getSiteBootstrap('en'),
    client.resolveRoute(path, 'en'),
  ])
  if (routeResult.status === 'rejected') {
    if (routeResult.reason instanceof PublicApiError && routeResult.reason.status === 404) {
      throw new PublicPageDataError('The requested public page does not exist.', 404)
    }
    throw new PublicPageDataError('The public route projection is temporarily unavailable.', 503)
  }
  if (bootstrapResult.status === 'rejected') {
    throw new PublicPageDataError('The public site bootstrap is temporarily unavailable.', 503)
  }
  if (routeResult.value.path !== path || routeResult.value.locale !== 'en') {
    throw new PublicPageDataError('The public route projection did not match the request.', 503)
  }
  const site = parseSiteBootstrap(bootstrapResult.value)
  const page = pageFromRoute(routeResult.value, site)
  return { page: await enrichPage(page, routeResult.value, client, options), site }
}
