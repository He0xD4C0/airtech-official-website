import { describe, expect, it } from 'vitest'
import { isCanonicalDiscoveryPath, isSearchablePublicPath } from './publicPaths'

describe('canonical public discovery paths', () => {
  it('accepts only route-backed content and product records', () => {
    expect(isCanonicalDiscoveryPath('content', '/en/solutions/verified-application')).toBe(true)
    expect(isCanonicalDiscoveryPath('content', '/en/resources/articles/verified-note')).toBe(true)
    expect(isCanonicalDiscoveryPath('content', '/en/resources/news/company-update')).toBe(true)
    expect(isCanonicalDiscoveryPath('content', '/en/products/axial')).toBe(true)
    expect(isCanonicalDiscoveryPath('product', '/en/products/axial/verified-model')).toBe(true)
    expect(isCanonicalDiscoveryPath('content', '/en/private/record')).toBe(false)
    expect(isCanonicalDiscoveryPath('content', '/en/products/axial/verified-model')).toBe(false)
    expect(isCanonicalDiscoveryPath('product', '/en/products/unknown/verified-model')).toBe(false)
    expect(isCanonicalDiscoveryPath('product', '/en/products/axial/not--canonical')).toBe(false)
  })

  it('allows database-discovered search destinations without admitting noindex tools', () => {
    expect(isSearchablePublicPath('content', '/en/request-a-quote')).toBe(true)
    expect(isSearchablePublicPath('content', '/en/products/selector')).toBe(true)
    expect(isSearchablePublicPath('product', '/en/products/selector')).toBe(false)
    expect(isSearchablePublicPath('content', '/en/search')).toBe(false)
    expect(isSearchablePublicPath('content', '/en/products/compare')).toBe(false)
    expect(isSearchablePublicPath('content', '/en/request-a-quote/product')).toBe(false)
  })
})
