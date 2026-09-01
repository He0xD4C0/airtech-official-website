export type DiscoveryEntityType = 'content' | 'product'

const slug = '[a-z0-9]+(?:-[a-z0-9]+)*'
const productDetail = new RegExp(`^/en/products/(centrifugal|axial|cross-flow|inline-duct|motors)/${slug}$`, 'u')
const publishedContent = [
  /^\/en$/u,
  new RegExp(`^/en/solutions(?:/${slug})?$`, 'u'),
  new RegExp(`^/en/technology(?:/${slug})?$`, 'u'),
  new RegExp(`^/en/resources/(articles|faqs|case-studies|downloads)(?:/${slug})?$`, 'u'),
  /^\/en\/company\/(about|contact)$/u,
  /^\/en\/(privacy|terms|cookie-settings)$/u,
]

const nonDiscoveryIndexablePages = new Set([
  '/en/products/selector',
  '/en/request-a-quote',
])

export function isPublishedProductPath(path: string): boolean {
  return productDetail.test(path)
}

export function isPublishedContentPath(path: string): boolean {
  return publishedContent.some((pattern) => pattern.test(path))
}

export function isCanonicalDiscoveryPath(entityType: DiscoveryEntityType, path: string): boolean {
  return entityType === 'product' ? isPublishedProductPath(path) : isPublishedContentPath(path)
}

export function isSearchablePublicPath(entityType: DiscoveryEntityType, path: string): boolean {
  return isCanonicalDiscoveryPath(entityType, path)
    || (entityType === 'content' && nonDiscoveryIndexablePages.has(path))
}
