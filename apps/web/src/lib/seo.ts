import type { PublicPageModel } from '@/types/content'

export function canonicalUrl(origin: string, page: Pick<PublicPageModel, 'canonicalPath'>): string {
  return `${origin}${page.canonicalPath}`
}

export function isIndexablePage(page: Pick<PublicPageModel, 'indexable' | 'dataState'>): boolean {
  return page.indexable && page.dataState !== 'placeholder'
}

export function robotsDirective(page: Pick<PublicPageModel, 'indexable' | 'dataState'>): string {
  return isIndexablePage(page)
    ? 'index,follow,max-image-preview:large'
    : 'noindex,follow,noarchive'
}

export function shouldEmitStructuredData(page: Pick<PublicPageModel, 'indexable' | 'dataState'>): boolean {
  return isIndexablePage(page)
}

export function openGraphType(page: Pick<PublicPageModel, 'publishedContent'>): 'article' | 'website' {
  return page.publishedContent?.kind === 'article' || page.publishedContent?.kind === 'caseStudy'
    ? 'article'
    : 'website'
}
