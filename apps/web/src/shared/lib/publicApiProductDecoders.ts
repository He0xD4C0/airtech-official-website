import type {
  Product,
  ProductPage,
  ProductSourceAssetDocument,
  PublicSearchPage,
} from '@airtek/contracts'
import { parseOpenApiSchema } from '@airtek/contracts'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/shared/lib/productPagination'
import type { PublishedProductListQuery, PublishedSearchQuery } from '@/shared/lib/publicApiTypes'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function nullableString(value: unknown): value is string | null {
  return value === null || typeof value === 'string'
}

function validFacet(facet: unknown): boolean {
  return isRecord(facet)
    && typeof facet.value === 'string'
    && Number.isInteger(facet.count)
    && Number(facet.count) >= 0
}

export function productResponse(value: unknown): Product {
  const validSpec = (spec: unknown) => isRecord(spec)
    && typeof spec.key === 'string'
    && typeof spec.label === 'string'
    && typeof spec.state === 'string'
  const validCurve = (curve: unknown) => isRecord(curve)
    && typeof curve.state === 'string'
    && Array.isArray(curve.points)
  if (!isRecord(value)
    || typeof value.id !== 'string'
    || typeof value.stableId !== 'string'
    || typeof value.slug !== 'string'
    || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(value.slug)
    || typeof value.title !== 'string'
    || !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(String(value.family))
    || value.locale !== 'en'
    || value.status !== 'published'
    || typeof value.indexable !== 'boolean'
    || !Number.isInteger(value.publishedRevision)
    || !Array.isArray(value.specifications)
    || !value.specifications.every(validSpec)
    || !Array.isArray(value.performanceCurves)
    || !value.performanceCurves.every(validCurve)) {
    throw new Error('The server returned an invalid published product.')
  }
  return parseOpenApiSchema<Product>('Product', value)
}

export function productPageResponse(value: unknown): ProductPage {
  if (!isRecord(value)
    || !Array.isArray(value.items)
    || (value.nextCursor !== null && typeof value.nextCursor !== 'string')
    || !Number.isInteger(value.total)
    || Number(value.total) < 0
    || !Array.isArray(value.familyCounts)
    || !value.familyCounts.every(validFacet)
    || !Array.isArray(value.motorTechnologyCounts)
    || !value.motorTechnologyCounts.every(validFacet)) {
    throw new Error('The server returned an invalid published product page.')
  }
  return parseOpenApiSchema<ProductPage>('ProductPage', {
    ...value,
    items: value.items.map(productResponse),
  })
}

export function searchPageResponse(value: unknown): PublicSearchPage {
  const validItem = (item: unknown) => isRecord(item)
    && ['content', 'product'].includes(String(item.entityType))
    && typeof item.entityId === 'string'
    && typeof item.title === 'string'
    && nullableString(item.summary)
    && typeof item.canonicalPath === 'string'
    && item.canonicalPath.startsWith('/en')
    && ['product', 'solution', 'technology', 'article', 'news', 'faq', 'caseStudy', 'download', 'company', 'page'].includes(String(item.displayType))
  if (!isRecord(value)
    || !Array.isArray(value.items)
    || !value.items.every(validItem)
    || (value.nextCursor !== null && typeof value.nextCursor !== 'string')
    || !Number.isInteger(value.total)
    || Number(value.total) < 0
    || !Array.isArray(value.typeCounts)
    || !value.typeCounts.every(validFacet)) {
    throw new Error('The server returned an invalid public search page.')
  }
  return parseOpenApiSchema<PublicSearchPage>('PublicSearchPage', value)
}

export function productSourceAssetDocumentResponse(value: unknown): ProductSourceAssetDocument {
  const validAsset = (asset: unknown) => isRecord(asset)
    && typeof asset.assetId === 'string'
    && typeof asset.usage === 'string'
    && typeof asset.originalName === 'string'
    && typeof asset.mediaType === 'string'
    && Number.isInteger(asset.byteSize)
    && Number(asset.byteSize) >= 0
    && typeof asset.sha256 === 'string'
    && /^[0-9a-f]{64}$/u.test(asset.sha256)
    && typeof asset.downloadUrl === 'string'
    && nullableString(asset.previewUrl)
  if (!isRecord(value)
    || typeof value.productId !== 'string'
    || !Number.isInteger(value.productRevision)
    || Number(value.productRevision) < 1
    || !Array.isArray(value.items)
    || !value.items.every(validAsset)) {
    throw new Error('The server returned invalid published product attachments.')
  }
  return parseOpenApiSchema<ProductSourceAssetDocument>('ProductSourceAssetDocument', value)
}

export function productListQuery(query: PublishedProductListQuery): PublishedProductListQuery {
  const limit = query.limit ?? PUBLIC_PRODUCT_PAGE_SIZE
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('The product page size is invalid.')
  if (query.cursor && !/^[A-Za-z0-9_-]{1,2048}$/u.test(query.cursor)) throw new Error('The product cursor is invalid.')
  if (query.q && (query.q.trim().length > 200 || /[\u0000-\u001f\u007f]/u.test(query.q))) throw new Error('The product search query is invalid.')
  if (query.motorTechnology && (query.motorTechnology.length > 120 || /[\u0000-\u001f\u007f]/u.test(query.motorTechnology))) throw new Error('The motor technology filter is invalid.')
  if (query.family && !['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(query.family)) throw new Error('The product family filter is invalid.')
  return {
    limit,
    ...(query.q?.trim() ? { q: query.q.trim() } : {}),
    ...(query.family ? { family: query.family } : {}),
    ...(query.motorTechnology?.trim() ? { motorTechnology: query.motorTechnology.trim() } : {}),
    ...(query.cursor ? { cursor: query.cursor } : {}),
  }
}

export function searchQuery(query: PublishedSearchQuery): PublishedSearchQuery {
  const limit = query.limit ?? 20
  if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw new Error('The search page size is invalid.')
  if (query.cursor && !/^[A-Za-z0-9_-]{1,2048}$/u.test(query.cursor)) throw new Error('The search cursor is invalid.')
  if (query.q && (query.q.trim().length > 200 || /[\u0000-\u001f\u007f]/u.test(query.q))) throw new Error('The public search query is invalid.')
  const allowedTypes = ['product', 'solution', 'technology', 'article', 'news', 'faq', 'caseStudy', 'download', 'company', 'page']
  if (query.type && !allowedTypes.includes(query.type)) throw new Error('The public search type is invalid.')
  return {
    limit,
    ...(query.q?.trim() ? { q: query.q.trim() } : {}),
    ...(query.type ? { type: query.type } : {}),
    ...(query.cursor ? { cursor: query.cursor } : {}),
  }
}
