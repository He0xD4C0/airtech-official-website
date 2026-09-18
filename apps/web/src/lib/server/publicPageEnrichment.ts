import type { Product, ProductContext, ProductFamily } from '@airtek/contracts'
import { PublicApiError } from '@/lib/api'
import type { createPublicApiClient } from '@/lib/publicApiClient'
import type { PublicDiscoveryEntryResponse, RouteProjectionResponse } from '@/lib/publicApiTypes'
import { isSearchablePublicPath } from '@/lib/publicPaths'
import { publishedNewsCard } from '@/lib/newsCard'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/lib/productPagination'
import type { CardEntry, PublicPageModel } from '@/types/content'
import {
  safeText,
  slugPattern,
} from './publicProjectionParsing'
import {
  breadcrumbsFromProjection,
  newsMetadataFrom,
  pageDescription,
  pageEyebrow,
  pageTitle,
  primaryCtaFromBlocks,
  relationshipEntriesFromProjection,
  sectionsFromBlocks,
} from './publicProjectionV2'
import { PublicPageDataError, type PublicPageDataOptions } from './publicPageDataTypes'

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

export async function enrichPage(
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
      const [product, assets] = await Promise.all([
        client.getProduct(match[2], family.code),
        client.getProductAssets(match[2], family.code),
      ])
      if (product.family !== family.code || product.slug !== match[2]) throw new PublicPageDataError('The published product route does not match the product record.', 404)
      if (!product.publishedRevision) throw new PublicPageDataError('The published product record has no revision.', 503)
      if (assets.productId !== product.id || assets.productRevision !== product.publishedRevision) throw new PublicPageDataError('The published product attachments do not match the product revision.', 503)
      page.category = family.slug
      page.slug = product.slug
      page.title = product.title
      page.metaTitle = product.seo.title?.trim() || product.title
      page.description = product.seo.description?.trim() || product.summary?.trim() || page.description
      page.eyebrow = product.model?.trim() || product.stableId
      page.indexable = page.indexable && product.indexable
      page.publishedProduct = product
      page.productAssets = assets.items
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
        const card = publishedNewsCard(entry)
        return card ? [card] : []
      })
      page.newsNextCursor = news.nextCursor
    } else if (page.kind === 'news-detail') {
      const slug = route.path.split('/').at(-1)
      if (!slug || !slugPattern.test(slug)) throw new PublicPageDataError('The News route is invalid.', 404)
      const news = await client.getNews(slug)
      if (news.content.slug !== slug
        || news.content.id !== route.entityId
        || news.content.publishedRevision !== route.publishedRevision) {
        throw new PublicPageDataError('The published News route does not match its CMS V2 record.', 503)
      }
      page.slug = slug
      const projection = news.content
      const title = pageTitle(projection)
      if (!title) throw new PublicPageDataError('The published News projection has no title.', 503)
      page.projection = projection
      page.blocks = projection.composition.blocks
      page.title = title
      page.metaTitle = safeText(projection.seo.title, 300) ?? page.title
      page.description = pageDescription(projection) ?? page.description
      page.eyebrow = pageEyebrow(projection) || news.category || page.eyebrow
      page.breadcrumbs = breadcrumbsFromProjection(projection, route.path)
      page.primaryCta = primaryCtaFromBlocks(projection)
      page.sections = sectionsFromBlocks(projection)
      page.entries = relationshipEntriesFromProjection(projection)
      page.indexable = page.indexable
        && !news.content.isPlaceholder
        && news.dataClass !== 'developmentFixture'
        && news.content.seo.indexable
      page.dataClass = news.dataClass
      page.dataState = news.content.isPlaceholder ? 'placeholder' : 'published'
      page.placeholderReason = news.content.isPlaceholder
        ? 'This published record is explicitly marked as placeholder content.'
        : undefined
      page.newsMetadata = newsMetadataFrom(projection) ?? {
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
    return page
  } catch (cause) {
    if (cause instanceof PublicPageDataError) throw cause
    if (cause instanceof PublicApiError && cause.status === 404) throw new PublicPageDataError('The requested published record does not exist.', 404)
    throw new PublicPageDataError('A required public projection is temporarily unavailable.', 503)
  }
}
