import type { PublicPageModel } from '@/types/content'

export function publicPageFixture(
  path: string,
  overrides: Partial<PublicPageModel> = {},
): PublicPageModel {
  const slug = path.split('/').filter(Boolean).at(-1)
  let kind: PublicPageModel['kind'] = 'detail'
  let collection: PublicPageModel['collection']
  if (path === '/en') kind = 'home'
  else if (path === '/en/resources/articles') { kind = 'collection'; collection = 'articles' }
  else if (path.startsWith('/en/resources/articles/')) collection = 'articles'
  else if (path === '/en/resources/faqs' || path.startsWith('/en/resources/faqs/')) kind = 'faq'
  else if (path === '/en/resources/downloads') { kind = 'downloads'; collection = 'downloads' }
  else if (path.startsWith('/en/resources/downloads/')) collection = 'downloads'
  else if (path === '/en/company/about') kind = 'about'
  else if (path === '/en/company/contact') kind = 'contact'

  const title = overrides.title ?? 'Fixture page'
  return {
    kind,
    canonicalPath: path,
    title,
    metaTitle: overrides.metaTitle ?? title,
    description: overrides.description ?? 'Fixture description.',
    eyebrow: overrides.eyebrow ?? 'Fixture',
    breadcrumbs: overrides.breadcrumbs ?? [{ label: title }],
    indexable: overrides.indexable ?? true,
    collection,
    slug: slug === 'en' ? undefined : slug,
    ...overrides,
  }
}
