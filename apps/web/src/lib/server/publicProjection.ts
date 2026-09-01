import type { ContentEntry, CursorPage, Product, ProductContext, ProductFamily } from '@airtek/contracts'
import type { PublicPageModel } from '@/types/content'
import { hasRenderableRichText } from '@/lib/richText'
import { isSearchablePublicPath } from '@/lib/publicPaths'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/lib/productPagination'
import { publishedDownloadListMetadata } from '@/lib/downloadResources'

export interface ProjectionLoadOptions {
  baseUrl?: string
  fetchImpl?: typeof fetch
  productSlug?: string
}

const productFamilySlugs: Record<ProductFamily, string> = {
  centrifugal: 'centrifugal',
  axial: 'axial',
  crossFlow: 'cross-flow',
  inlineDuct: 'inline-duct',
  motors: 'motors',
}

function internalApiBaseUrl() {
  return process.env.PUBLIC_API_INTERNAL_URL || 'http://localhost:8080/api/public/v1'
}

function normalizedBaseUrl(value: string) {
  const url = new URL(value)
  if (!['http:', 'https:'].includes(url.protocol)) throw new Error('Unsupported internal API protocol.')
  return url.toString().replace(/\/$/, '')
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

const factStates = new Set(['verified', 'missing', 'notApplicable', 'notTested', 'confidential', 'pendingVerification'])

function isNullableString(value: unknown): boolean {
  return value === null || typeof value === 'string'
}

function isNullableFiniteNumber(value: unknown): boolean {
  return value === null || (typeof value === 'number' && Number.isFinite(value))
}

function isCanonicalSlug(value: unknown, allowIndex = false): value is string {
  return typeof value === 'string'
    && (allowIndex || value !== 'index')
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(value)
}

function isSpecification(value: unknown): boolean {
  return isRecord(value)
    && typeof value.key === 'string'
    && typeof value.label === 'string'
    && factStates.has(String(value.state))
    && isNullableString(value.unit)
    && isNullableString(value.operatingCondition)
    && isNullableString(value.sourceReference)
}

function isPerformanceCurve(value: unknown): boolean {
  return isRecord(value)
    && typeof value.airflowUnit === 'string'
    && typeof value.pressureUnit === 'string'
    && typeof value.sourceReference === 'string'
    && factStates.has(String(value.state))
    && isNullableFiniteNumber(value.densityKgM3)
    && isNullableFiniteNumber(value.speedRpm)
    && isNullableString(value.voltage)
    && isNullableString(value.testMethod)
    && Array.isArray(value.points)
    && value.points.every((point) => isRecord(point)
      && typeof point.airflow === 'number'
      && Number.isFinite(point.airflow)
      && point.airflow >= 0
      && typeof point.pressure === 'number'
      && Number.isFinite(point.pressure)
      && point.pressure >= 0)
}

function isPublishedContent(value: unknown): value is ContentEntry {
  return isRecord(value)
    && typeof value.id === 'string'
    && typeof value.kind === 'string'
    && isCanonicalSlug(value.slug, true)
    && typeof value.title === 'string'
    && Boolean(value.title.trim())
    && isNullableString(value.summary)
    && isRecord(value.body)
    && value.body.schemaVersion === 1
    && isRecord(value.body.doc)
    && isRecord(value.seo)
    && isNullableString(value.seo.title)
    && isNullableString(value.seo.description)
    && isNullableString(value.seo.canonicalPath)
    && typeof value.seo.indexable === 'boolean'
    && value.status === 'published'
    && typeof value.isPlaceholder === 'boolean'
    && Number.isInteger(value.currentRevision)
    && Number.isInteger(value.publishedRevision)
    && Number(value.publishedRevision) > 0
    && typeof value.updatedAt === 'string'
    && Number.isFinite(Date.parse(value.updatedAt))
}

function isPublishedProduct(value: unknown): value is Product {
  return isRecord(value)
    && typeof value.id === 'string'
    && typeof value.stableId === 'string'
    && Boolean(value.stableId.trim())
    && isCanonicalSlug(value.slug)
    && typeof value.title === 'string'
    && Boolean(value.title.trim())
    && typeof value.family === 'string'
    && Object.hasOwn(productFamilySlugs, value.family)
    && value.locale === 'en'
    && value.status === 'published'
    && typeof value.indexable === 'boolean'
    && isNullableString(value.model)
    && isNullableString(value.subtype)
    && isNullableString(value.motorTechnology)
    && isNullableString(value.summary)
    && typeof value.sourceSnapshotId === 'string'
    && typeof value.sourceRevision === 'string'
    && Number.isInteger(value.currentRevision)
    && Number.isInteger(value.publishedRevision)
    && Number(value.publishedRevision) > 0
    && Array.isArray(value.specifications)
    && value.specifications.every(isSpecification)
    && Array.isArray(value.performanceCurves)
    && value.performanceCurves.every(isPerformanceCurve)
    && typeof value.updatedAt === 'string'
    && Number.isFinite(Date.parse(value.updatedAt))
}

function isPublishedProductPage(value: unknown): value is CursorPage<Product> {
  return isRecord(value)
    && Array.isArray(value.items)
    && value.items.every(isPublishedProduct)
    && (value.nextCursor === null
      || (typeof value.nextCursor === 'string' && /^[A-Za-z0-9_-]{1,2048}$/u.test(value.nextCursor)))
}

function contentResource(page: PublicPageModel): { kind: string; slug: string } | undefined {
  if (page.kind === 'home') return { kind: 'home', slug: 'home' }
  if (page.kind === 'collection' && page.collection) {
    const kind = page.collection === 'cases' ? 'case-studies' : page.collection
    return { kind, slug: 'index' }
  }
  if (page.kind === 'detail' && page.collection && page.slug) {
    const kind = page.collection === 'cases' ? 'case-studies' : page.collection
    return { kind, slug: page.slug }
  }
  if (page.kind === 'faq') return { kind: 'faqs', slug: page.slug || 'index' }
  if (page.kind === 'downloads') return { kind: 'downloads', slug: page.slug || 'index' }
  if (page.kind === 'about') return { kind: 'company', slug: 'about' }
  if (page.kind === 'contact') return { kind: 'company', slug: 'contact' }
  if (page.kind === 'legal') return { kind: 'legal', slug: page.canonicalPath.split('/').at(-1) || 'index' }
  return undefined
}

const contentKinds: Record<string, ContentEntry['kind']> = {
  home: 'home',
  solutions: 'solution',
  technology: 'technology',
  articles: 'article',
  faqs: 'faq',
  'case-studies': 'caseStudy',
  downloads: 'download',
  company: 'company',
  legal: 'legal',
}

function matchesContentResource(value: ContentEntry, resource: { kind: string; slug: string }): boolean {
  return value.kind === contentKinds[resource.kind]
    && value.slug === resource.slug
    && value.locale === 'en'
}

interface DiscoveryEntryProjection {
  entityType: 'content' | 'product'
  entityId: string
  path: string
  locale: string
  title?: string
  summary?: string | null
  updatedAt: string
}

interface DiscoveryProjection {
  generatedAt: string
  entries: DiscoveryEntryProjection[]
}

function isDiscoveryProjection(value: unknown): value is DiscoveryProjection {
  if (!isRecord(value) || typeof value.generatedAt !== 'string' || !Array.isArray(value.entries)) return false
  return value.entries.every((entry) => isRecord(entry)
    && (entry.entityType === 'content' || entry.entityType === 'product')
    && typeof entry.entityId === 'string'
    && typeof entry.path === 'string'
    && typeof entry.locale === 'string'
    && (entry.title === undefined || typeof entry.title === 'string')
    && (entry.summary === undefined || entry.summary === null || typeof entry.summary === 'string')
    && typeof entry.updatedAt === 'string')
}

function discoveryPrefix(page: PublicPageModel): string | undefined {
  if (page.kind === 'collection') {
    if (page.collection === 'solutions') return '/en/solutions'
    if (page.collection === 'technology') return '/en/technology'
    if (page.collection === 'articles') return '/en/resources/articles'
    if (page.collection === 'cases') return '/en/resources/case-studies'
  }
  if (page.kind === 'faq') return '/en/resources/faqs'
  if (page.kind === 'downloads') return '/en/resources/downloads'
  return undefined
}

function validDiscoveredDetailPath(path: string, prefix: string): boolean {
  if (!path.startsWith(`${prefix}/`)) return false
  const slug = path.slice(prefix.length + 1)
  return slug !== 'index'
    && slug.length <= 180
    && !slug.startsWith('-')
    && !slug.endsWith('-')
    && !slug.includes('--')
    && /^[a-z0-9-]+$/.test(slug)
}

async function enrichDownloadEntries(
  entries: NonNullable<PublicPageModel['entries']>,
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<NonNullable<PublicPageModel['entries']>> {
  return Promise.all(entries.map(async (entry) => {
    try {
      const value = await getJson(`${baseUrl}/content/downloads/${encodeURIComponent(entry.slug)}?locale=en`, fetchImpl)
      if (!isPublishedContent(value)
        || !matchesContentResource(value, { kind: 'downloads', slug: entry.slug })) return entry
      const download = publishedDownloadListMetadata(value)
      return download ? { ...entry, download } : entry
    } catch {
      return entry
    }
  }))
}

async function loadDiscoveryEntries(
  page: PublicPageModel,
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<PublicPageModel> {
  const prefix = discoveryPrefix(page)
  if (!prefix) return page
  try {
    const value = await getJson(`${baseUrl}/discovery`, fetchImpl)
    if (!isDiscoveryProjection(value)) throw new Error('Published discovery response was invalid.')
    const seen = new Set<string>()
    const entries = value.entries.flatMap((entry) => {
      const title = entry.title?.trim()
      if (entry.entityType !== 'content'
        || entry.locale !== 'en'
        || !title
        || seen.has(entry.path)
        || !validDiscoveredDetailPath(entry.path, prefix)) return []
      seen.add(entry.path)
      return [{
        slug: entry.path.slice(prefix.length + 1),
        title,
        summary: entry.summary?.trim() ?? '',
        eyebrow: page.kind === 'faq' ? 'FAQ' : page.kind === 'downloads' ? 'Download' : page.eyebrow,
        href: entry.path,
      }]
    })
    return {
      ...page,
      entries: page.kind === 'downloads'
        ? await enrichDownloadEntries(entries, baseUrl, fetchImpl)
        : entries,
    }
  } catch {
    return { ...page, entries: [] }
  }
}

function searchEntryType(entry: DiscoveryEntryProjection): string {
  if (entry.entityType === 'product') return 'Product'
  if (entry.path.startsWith('/en/solutions/')) return 'Solution'
  if (entry.path.startsWith('/en/technology/')) return 'Technology'
  if (entry.path.startsWith('/en/resources/articles/')) return 'Article'
  if (entry.path.startsWith('/en/resources/faqs/')) return 'FAQ'
  if (entry.path.startsWith('/en/resources/case-studies/')) return 'Case study'
  if (entry.path.startsWith('/en/resources/downloads/')) return 'Download'
  if (entry.path.startsWith('/en/company/')) return 'Company'
  return 'Page'
}

async function loadSearchEntries(
  page: PublicPageModel,
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<PublicPageModel> {
  try {
    const value = await getJson(`${baseUrl}/discovery`, fetchImpl)
    if (!isDiscoveryProjection(value)) throw new Error('Published discovery response was invalid.')
    const seen = new Set<string>()
    const entries = value.entries.flatMap((entry) => {
      const title = entry.title?.trim()
      if (entry.locale !== 'en'
        || !title
        || seen.has(entry.path)
        || !isSearchablePublicPath(entry.entityType, entry.path)) return []
      seen.add(entry.path)
      return [{
        slug: entry.path.split('/').at(-1) || 'page',
        title,
        summary: entry.summary?.trim() ?? '',
        eyebrow: searchEntryType(entry),
        href: entry.path,
      }]
    })
    return { ...page, entries }
  } catch {
    return { ...page, entries: [] }
  }
}

function fallback(page: PublicPageModel, reason: string): PublicPageModel {
  const safe = { ...page }
  delete safe.publishedContent
  delete safe.publishedProducts
  delete safe.productNextCursor
  delete safe.publishedProduct
  delete safe.productContext
  return {
    ...safe,
    indexable: false,
    dataState: 'placeholder',
    placeholderReason: reason,
  }
}

const familyQueries: Record<string, ProductFamily> = {
  centrifugal: 'centrifugal',
  axial: 'axial',
  'cross-flow': 'crossFlow',
  'inline-duct': 'inlineDuct',
  motors: 'motors',
}

async function loadProducts(
  page: PublicPageModel,
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<PublicPageModel> {
  try {
    const requestedFamily = page.category ? familyQueries[page.category] : undefined
    if (page.category && !requestedFamily) throw new Error('The requested product family is invalid.')
    const query = new URLSearchParams({ limit: String(PUBLIC_PRODUCT_PAGE_SIZE) })
    if (requestedFamily) query.set('family', requestedFamily)
    const value = await getJson(`${baseUrl}/products?${query.toString()}`, fetchImpl)
    if (!isPublishedProductPage(value)) throw new Error('Published product list response was invalid.')
    const products = value.items.filter((product) => !requestedFamily || product.family === requestedFamily)
    const hasPublishedProducts = products.length > 0
    return {
      ...page,
      indexable: page.indexable && hasPublishedProducts,
      dataState: hasPublishedProducts ? 'published' : 'placeholder',
      placeholderReason: hasPublishedProducts
        ? undefined
        : 'No published Product Master records are available in this catalog view.',
      publishedProducts: products,
      productNextCursor: value.nextCursor ?? null,
    }
  } catch {
    return fallback(page, 'The published product catalog is unavailable. No product records have been inferred.')
  }
}

async function getJson(url: string, fetchImpl: typeof fetch) {
  const response = await fetchImpl(url, {
    headers: { Accept: 'application/json' },
    signal: AbortSignal.timeout(3_000),
  })
  if (!response.ok) throw new Error(`Published projection returned ${response.status}.`)
  return response.json() as Promise<unknown>
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

async function loadProduct(
  page: PublicPageModel,
  productSlug: string,
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<PublicPageModel> {
  try {
    const value = await getJson(`${baseUrl}/products/${encodeURIComponent(productSlug)}`, fetchImpl)
    if (!isPublishedProduct(value)) throw new Error('Published product response was invalid.')
    if (page.kind === 'product-detail') {
      if (value.slug !== page.slug || productFamilySlugs[value.family] !== page.category) {
        throw new Error('Published product did not match the requested route.')
      }
    }
    const context = productContext(value)
    if (page.kind === 'rfq-form') {
      return {
        ...page,
        dataState: 'published',
        productContext: context,
        placeholderReason: context ? undefined : 'The published product is missing the exact model required for a Product RFQ.',
      }
    }
    return {
      ...page,
      title: value.title,
      metaTitle: `${value.title} | AIRTEKPOWER`,
      description: value.summary || 'Published AIRTEKPOWER product record.',
      eyebrow: value.model || value.stableId,
      indexable: value.indexable,
      dataState: 'published',
      placeholderReason: undefined,
      publishedProduct: value,
      productContext: context,
      breadcrumbs: page.breadcrumbs.map((item, index) => index === page.breadcrumbs.length - 1 ? { label: value.title } : item),
    }
  } catch {
    return fallback(page, 'The published product projection is unavailable. No product values or RFQ context have been inferred.')
  }
}

async function loadContent(
  page: PublicPageModel,
  resource: { kind: string; slug: string },
  baseUrl: string,
  fetchImpl: typeof fetch,
): Promise<PublicPageModel> {
  try {
    const value = await getJson(`${baseUrl}/content/${resource.kind}/${encodeURIComponent(resource.slug)}?locale=en`, fetchImpl)
    if (!isPublishedContent(value)) throw new Error('Published content response was invalid.')
    if (!matchesContentResource(value, resource)) throw new Error('Published content did not match the requested route.')
    const canonicalPath = value.seo.canonicalPath === page.canonicalPath ? value.seo.canonicalPath : page.canonicalPath
    const isPlaceholder = value.isPlaceholder === true
    const missingRequiredBody = page.requiresPublishedContent === true && !hasRenderableRichText(value.body)
    return {
      ...page,
      canonicalPath,
      title: value.title,
      metaTitle: value.seo.title || `${value.title} | AIRTEKPOWER`,
      description: value.seo.description || value.summary || page.description,
      indexable: !isPlaceholder && !missingRequiredBody && value.seo.indexable,
      dataState: isPlaceholder || missingRequiredBody ? 'placeholder' : 'published',
      placeholderReason: isPlaceholder
        ? 'This published projection is explicitly marked as placeholder content.'
        : missingRequiredBody ? 'The published record has no renderable schema-v1 body and remains excluded from indexing.' : undefined,
      requiresPublishedContent: missingRequiredBody,
      publishedContent: value,
      breadcrumbs: page.breadcrumbs.map((item, index) => index === page.breadcrumbs.length - 1 ? { label: value.title } : item),
    }
  } catch {
    return fallback(page, 'The published content projection is unavailable. This safe placeholder is excluded from indexing and structured data.')
  }
}

export async function loadPublishedProjection(
  page: PublicPageModel,
  options: ProjectionLoadOptions = {},
): Promise<PublicPageModel> {
  const fetchImpl = options.fetchImpl ?? fetch
  const baseUrl = normalizedBaseUrl(options.baseUrl ?? internalApiBaseUrl())

  if (page.kind === 'catalog') return loadProducts(page, baseUrl, fetchImpl)
  if (page.kind === 'search') return loadSearchEntries(page, baseUrl, fetchImpl)

  const requestedProduct = page.kind === 'product-detail' ? page.slug : options.productSlug
  if ((page.kind === 'product-detail' || (page.kind === 'rfq-form' && page.rfqType === 'product')) && requestedProduct) {
    return loadProduct(page, requestedProduct, baseUrl, fetchImpl)
  }
  if (page.kind === 'rfq-form' && page.rfqType === 'product') {
    return fallback(page, 'A Product RFQ requires a published product route. Choose a product or use the Fan Selection RFQ.')
  }

  const resource = contentResource(page)
  if (!resource) return page
  const content = await loadContent(page, resource, baseUrl, fetchImpl)
  return loadDiscoveryEntries(content, baseUrl, fetchImpl)
}
