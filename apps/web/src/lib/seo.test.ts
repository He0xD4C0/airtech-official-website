import { describe, expect, it } from 'vitest'
import { isIndexablePage, openGraphType, robotsDirective, shouldEmitStructuredData } from './seo'

describe('public SEO policy', () => {
  it('never indexes or emits schema for a placeholder state', () => {
    const placeholder = { indexable: true, dataState: 'placeholder' as const }
    expect(isIndexablePage(placeholder)).toBe(false)
    expect(robotsDirective(placeholder)).toBe('noindex,follow,noarchive')
    expect(shouldEmitStructuredData(placeholder)).toBe(false)
  })

  it('indexes only a non-placeholder page explicitly marked indexable', () => {
    const published = { indexable: true, dataState: 'published' as const }
    expect(robotsDirective(published)).toBe('index,follow,max-image-preview:large')
    expect(shouldEmitStructuredData(published)).toBe(true)
  })

  it('uses article Open Graph semantics only for supported published editorial records', () => {
    expect(openGraphType({ publishedContent: { kind: 'article' } as never })).toBe('article')
    expect(openGraphType({ publishedContent: { kind: 'caseStudy' } as never })).toBe('article')
    expect(openGraphType({ publishedContent: undefined })).toBe('website')
  })
})
