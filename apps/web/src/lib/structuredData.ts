import { canonicalUrl, isIndexablePage } from '@/lib/seo'
import { extractPublishedArticleMetadata, extractPublishedFaqContent } from '@/lib/publishedContent'
import type { PublicPageModel, PublicSiteBootstrap } from '@/types/content'

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

function faqPageSchema(page: PublicPageModel, origin: string, hasSite: boolean): SchemaNode | undefined {
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
    ...(page.description ? { description: page.description } : {}),
    url: canonicalUrl(origin, page),
    inLanguage: content.locale,
    ...(hasSite ? { isPartOf: { '@id': `${origin}/#website` } } : {}),
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

function entitySchema(page: PublicPageModel, origin: string, hasSite: boolean): SchemaNode {
  const canonical = canonicalUrl(origin, page)
  const product = page.publishedProduct
  if (product) {
    const description = product.summary?.trim() || page.description
    return {
      '@type': 'Product',
      name: product.title,
      ...(description ? { description } : {}),
      productID: product.stableId,
      ...(product.model ? { sku: product.model } : {}),
      category: product.family,
      ...(hasSite ? { brand: { '@id': `${origin}/#organization` } } : {}),
      url: canonical,
    }
  }

  const faq = faqPageSchema(page, origin, hasSite)
  if (faq) return faq

  const content = page.publishedContent
  if (content?.kind === 'article' || content?.kind === 'caseStudy') {
    const metadata = extractPublishedArticleMetadata(content)
    return {
      '@type': 'Article',
      headline: content.title,
      ...((content.summary?.trim() || page.description) ? { description: content.summary?.trim() || page.description } : {}),
      inLanguage: content.locale,
      dateModified: content.updatedAt,
      ...(metadata.publishedAt ? { datePublished: metadata.publishedAt } : {}),
      ...(metadata.category ? { articleSection: metadata.category } : {}),
      ...(metadata.author && metadata.authorType ? {
        author: { '@type': metadata.authorType, name: metadata.author },
      } : {}),
      ...(hasSite ? { publisher: { '@id': `${origin}/#organization` } } : {}),
      mainEntityOfPage: canonical,
    }
  }

  return {
    '@type': 'WebPage',
    name: page.title,
    ...(page.description ? { description: page.description } : {}),
    url: canonical,
    inLanguage: 'en',
    ...(hasSite ? { isPartOf: { '@id': `${origin}/#website` } } : {}),
  }
}

export function buildPublicStructuredData(page: PublicPageModel, origin: string, site?: PublicSiteBootstrap): SchemaNode {
  const breadcrumb = breadcrumbSchema(page, origin)
  const organizationUrl = site?.organization.url
    ? (site.organization.url.startsWith('/') ? `${origin}${site.organization.url}` : site.organization.url)
    : (site ? `${origin}${site.homePath}` : undefined)
  const organizationLogo = site?.organization.logoUrl
    ? (site.organization.logoUrl.startsWith('/') ? `${origin}${site.organization.logoUrl}` : site.organization.logoUrl)
    : undefined
  return {
    '@context': 'https://schema.org',
    '@graph': [
      ...(site ? [{
        '@type': 'Organization',
        '@id': `${origin}/#organization`,
        name: site.organization.name,
        ...(organizationUrl ? { url: organizationUrl } : {}),
        ...(organizationLogo ? { logo: organizationLogo } : {}),
      }, {
        '@type': 'WebSite',
        '@id': `${origin}/#website`,
        name: site.brandName,
        url: `${origin}${site.homePath}`,
        inLanguage: 'en',
        publisher: { '@id': `${origin}/#organization` },
      }] : []),
      ...(breadcrumb ? [breadcrumb] : []),
      entitySchema(page, origin, Boolean(site)),
    ],
  }
}
