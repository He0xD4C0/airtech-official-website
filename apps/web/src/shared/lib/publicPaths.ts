export type DiscoveryEntityType = 'content' | 'product'

const slug = '[a-z0-9]+(?:-[a-z0-9]+)*'
const productDetail = new RegExp(`^/en/products/(centrifugal|axial|cross-flow|inline-duct|motors)/${slug}$`, 'u')
const publicContentPaths = [
  /^\/en$/u,
  new RegExp(`^/en/products(?:/(centrifugal|axial|cross-flow|inline-duct|motors|selector))?$`, 'u'),
  new RegExp(`^/en/solutions(?:/${slug})?$`, 'u'),
  new RegExp(`^/en/technology(?:/${slug})?$`, 'u'),
  new RegExp(`^/en/resources/(articles|news|faqs|case-studies|downloads)(?:/${slug})?$`, 'u'),
  /^\/en\/company\/(about|contact)$/u,
  /^\/en\/request-a-quote$/u,
  /^\/en\/(privacy|terms|cookie-settings)$/u,
]

export function isPublishedProductPath(path: string): boolean {
  return productDetail.test(path)
}

export function isPublishedContentPath(path: string): boolean {
  return publicContentPaths.some((pattern) => pattern.test(path))
}

export function isCanonicalDiscoveryPath(entityType: DiscoveryEntityType, path: string): boolean {
  return entityType === 'product' ? isPublishedProductPath(path) : isPublishedContentPath(path)
}

export function isSearchablePublicPath(entityType: DiscoveryEntityType, path: string): boolean {
  return isCanonicalDiscoveryPath(entityType, path)
}
