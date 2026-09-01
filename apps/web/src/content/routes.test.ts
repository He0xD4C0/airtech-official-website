import { describe, expect, it } from 'vitest'
import { resolvePublicRoute } from './routes'

describe('public route registry', () => {
  it('resolves every approved top-level route', () => {
    expect(resolvePublicRoute('/en').kind).toBe('home')
    expect(resolvePublicRoute('/en/products').kind).toBe('catalog')
    expect(resolvePublicRoute('/en/products/selector').kind).toBe('selector')
    expect(resolvePublicRoute('/en/solutions').kind).toBe('collection')
    expect(resolvePublicRoute('/en/request-a-quote/project').rfqType).toBe('project')
  })

  it('covers the complete public SSR route contract', () => {
    const routes: Array<[string, string]> = [
      ['/en', 'home'],
      ['/en/products', 'catalog'],
      ['/en/products/centrifugal', 'catalog'],
      ['/en/products/axial/controlled-model', 'product-detail'],
      ['/en/products/selector', 'selector'],
      ['/en/solutions', 'collection'],
      ['/en/solutions/hvac', 'detail'],
      ['/en/technology', 'collection'],
      ['/en/technology/control', 'detail'],
      ['/en/resources/articles', 'collection'],
      ['/en/resources/articles/reading-a-fan-curve', 'detail'],
      ['/en/resources/faqs', 'faq'],
      ['/en/resources/faqs/technical', 'faq'],
      ['/en/resources/case-studies', 'collection'],
      ['/en/resources/case-studies/verified-case-record-preview', 'detail'],
      ['/en/resources/downloads', 'downloads'],
      ['/en/resources/downloads/controlled-file', 'detail'],
      ['/en/company/about', 'about'],
      ['/en/company/contact', 'contact'],
      ['/en/request-a-quote', 'rfq-router'],
      ['/en/request-a-quote/product', 'rfq-form'],
      ['/en/request-a-quote/selection', 'rfq-form'],
      ['/en/request-a-quote/project', 'rfq-form'],
      ['/en/request-a-quote/replacement', 'rfq-form'],
      ['/en/search', 'search'],
      ['/en/privacy', 'legal'],
      ['/en/terms', 'legal'],
      ['/en/cookie-settings', 'legal'],
    ]

    for (const [path, kind] of routes) {
      const route = resolvePublicRoute(path)
      expect(route.kind, path).toBe(kind)
      expect(route.canonicalPath, path).toBe(path)
      expect(route.title, path).toBeTruthy()
      expect(route.description, path).toBeTruthy()
    }
  })

  it('keeps placeholders out of the index', () => {
    expect(resolvePublicRoute('/en/products/axial/product-record-preview').indexable).toBe(false)
    expect(resolvePublicRoute('/en/resources/case-studies/verified-case-record-preview').indexable).toBe(false)
  })

  it('keeps product technology separate from fan form', () => {
    expect(resolvePublicRoute('/en/products/motors').category).toBe('motors')
    expect(() => resolvePublicRoute('/en/products/ec')).toThrow()
  })

  it('never routes the isolated admin namespace', () => {
    expect(() => resolvePublicRoute('/admin')).toThrow()
    expect(() => resolvePublicRoute('/en/admin')).toThrow()
  })

  it('reserves valid CMS detail slugs as noindex placeholders', () => {
    for (const path of [
      '/en/solutions/new-application',
      '/en/technology/new-topic',
      '/en/resources/articles/new-article',
      '/en/resources/case-studies/new-case',
      '/en/resources/faqs/new-category',
      '/en/resources/downloads/new-resource',
    ]) {
      const resolved = resolvePublicRoute(path)
      expect(resolved.indexable, path).toBe(false)
      expect(resolved.dataState, path).toBe('placeholder')
      expect(resolved.requiresPublishedContent, path).toBe(true)
    }
  })

  it('rejects invalid slugs and unknown public prefixes', () => {
    expect(() => resolvePublicRoute('/en/solutions/not--valid')).toThrow()
    expect(() => resolvePublicRoute('/en/solutions/index')).toThrow()
    expect(() => resolvePublicRoute('/en/solutions/new/topic')).toThrow()
    expect(() => resolvePublicRoute('/en/resources/unknown/new-topic')).toThrow()
    expect(() => resolvePublicRoute('/en/products/axial/not--valid')).toThrow()
  })
})
