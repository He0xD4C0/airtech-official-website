import { describe, expect, it } from 'vitest'
import type { ContentTemplateDefinition } from '@airtek/contracts'
import { canonicalPathForDraft, routePatternUsesSlug } from '@/features/content/services/canonicalPath'

type RouteTemplate = Pick<ContentTemplateDefinition, 'routable' | 'routePattern'>

function template(routePattern: string | null, routable = true): RouteTemplate {
  return { routable, routePattern }
}

describe('canonicalPathForDraft', () => {
  const draft = { locale: 'en', slug: 'spring-update' }

  it('interpolates the server registry pattern without a kind route mirror', () => {
    expect(canonicalPathForDraft(
      draft,
      template('/{locale}/resources/articles/{slug}'),
    )).toBe('/en/resources/articles/spring-update')
  })

  it('supports registry routes that do not contain a slug', () => {
    expect(canonicalPathForDraft(
      { locale: 'en', slug: null },
      template('/{locale}/company/about'),
    )).toBe('/en/company/about')
    expect(routePatternUsesSlug('/{locale}/company/about')).toBe(false)
    expect(routePatternUsesSlug('/{locale}/resources/news/{slug}')).toBe(true)
  })

  it('returns null for non-routable or incomplete template definitions', () => {
    expect(canonicalPathForDraft(draft, template(null, false))).toBeNull()
    expect(canonicalPathForDraft(draft, template('/{locale}/{slug}', false))).toBeNull()
    expect(canonicalPathForDraft(draft, template(null))).toBeNull()
    expect(canonicalPathForDraft(draft, null)).toBeNull()
  })

  it('rejects missing or unsafe interpolation values', () => {
    const route = template('/{locale}/resources/news/{slug}')
    expect(canonicalPathForDraft({ locale: 'en', slug: null }, route)).toBeNull()
    expect(canonicalPathForDraft({ locale: 'en', slug: '../draft' }, route)).toBeNull()
    expect(canonicalPathForDraft({ locale: '../en', slug: 'draft' }, route)).toBeNull()
  })

  it.each([
    'https://example.test/{locale}/{slug}',
    '//example.test/{locale}/{slug}',
    '/{locale}/../{slug}',
    '/{locale}/{kind}/{slug}',
    '/{locale}/news?slug={slug}',
    '/{locale}/{slug}/{slug}',
    '/{locale}/',
  ])('rejects unknown or unsafe route pattern %s', (routePattern) => {
    expect(canonicalPathForDraft(draft, template(routePattern))).toBeNull()
    expect(routePatternUsesSlug(routePattern)).toBe(false)
  })
})
