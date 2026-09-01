import { canonicalUrl, isIndexablePage } from '@/lib/seo'
import { extractPublishedArticleMetadata, extractPublishedFaqContent } from '@/lib/publishedContent'
import type { PublicPageModel } from '@/types/content'

type SchemaNode = Record<string, unknown>

function breadcrumbSchema(page: PublicPageModel, origin: string): SchemaNode | undefined {
  if (page.breadcrumbs.length <= 1) return undefined
  return {
    '@type': 'BreadcrumbList',
    itemListElement: page.breadcrumbs.map((item, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: item.label,
      item: `${origin}${item.href ?? page.canonicalPath}`,
    })),
  }
}

function faqPageSchema(page: PublicPageModel, origin: string): SchemaNode | undefined {
  const content = page.publishedContent
  if (page.kind !== 'faq'
    || page.dataState !== 'published'
    || !isIndexablePage(page)
    || content?.kind !== 'faq'
    || content.status !== 'published'
    || content.isPlaceholder) return undefined

  const faq = extractPublishedFaqContent(content.body)
  if (!faq.complete || !faq.items.length) return undefined
  return {
    '@type': 'FAQPage',
    name: page.title,
    description: page.description,
    url: canonicalUrl(origin, page),
    inLanguage: content.locale,
    isPartOf: { '@id': `${origin}/#website` },
    mainEntity: faq.items.map((item) => ({
      '@type': 'Question',
      name: item.question,
      acceptedAnswer: {
        '@type': 'Answer',
        text: item.answer,
      },
    })),
  }
}

function entitySchema(page: PublicPageModel, origin: string): SchemaNode {
  const canonical = canonicalUrl(origin, page)
  const product = page.publishedProduct
  if (product) {
    return {
      '@type': 'Product',
      name: product.title,
      description: product.summary || page.description,
      productID: product.stableId,
      ...(product.model ? { sku: product.model } : {}),
      category: product.family,
      brand: { '@id': `${origin}/#organization` },
      url: canonical,
    }
  }

  const faq = faqPageSchema(page, origin)
  if (faq) return faq

  const content = page.publishedContent
  if (content?.kind === 'article' || content?.kind === 'caseStudy') {
    const metadata = extractPublishedArticleMetadata(content)
    return {
      '@type': 'Article',
      headline: content.title,
      description: content.summary || page.description,
      inLanguage: content.locale,
      dateModified: content.updatedAt,
      ...(metadata.publishedAt ? { datePublished: metadata.publishedAt } : {}),
      ...(metadata.category ? { articleSection: metadata.category } : {}),
      ...(metadata.author && metadata.authorType ? {
        author: { '@type': metadata.authorType, name: metadata.author },
      } : {}),
      publisher: { '@id': `${origin}/#organization` },
      mainEntityOfPage: canonical,
    }
  }

  return {
    '@type': 'WebPage',
    name: page.title,
    description: page.description,
    url: canonical,
    inLanguage: 'en',
    isPartOf: { '@id': `${origin}/#website` },
  }
}

export function buildPublicStructuredData(page: PublicPageModel, origin: string): SchemaNode {
  const breadcrumb = breadcrumbSchema(page, origin)
  return {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'Organization',
        '@id': `${origin}/#organization`,
        name: 'AIRTEKPOWER',
        url: `${origin}/en`,
      },
      {
        '@type': 'WebSite',
        '@id': `${origin}/#website`,
        name: 'AIRTEKPOWER',
        url: `${origin}/en`,
        inLanguage: 'en',
        publisher: { '@id': `${origin}/#organization` },
      },
      ...(breadcrumb ? [breadcrumb] : []),
      entitySchema(page, origin),
    ],
  }
}
