import { canonicalUrl, isIndexablePage } from '@/lib/seo'
import { asRichTextNode, richTextPlainText } from '@/lib/richText'
import type { PublicPageModel, PublicSiteBootstrap } from '@/types/content'
import { articleMetadataFrom } from './server/publicProjectionV2'

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
  const projection = page.projection
  if (!projection
    || page.kind !== 'faq'
    || page.dataState !== 'published'
    || !isIndexablePage(page)
    || projection.isPlaceholder
    || projection.typeFields.type !== 'faq') return undefined
  const items = projection.typeFields.items.map((item) => ({
    question: item.question.trim(),
    answer: richTextPlainText(asRichTextNode(item.answer)).trim(),
  }))
  if (!items.length || items.some((item) => !item.question || !item.answer)) return undefined
  return {
    '@type': 'FAQPage',
    name: page.title,
    ...(page.description ? { description: page.description } : {}),
    url: canonicalUrl(origin, page),
    inLanguage: projection.locale,
    ...(hasSite ? { isPartOf: { '@id': `${origin}/#website` } } : {}),
    mainEntity: items.map((item) => ({
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

  const projection = page.projection
  if (projection && (projection.kind === 'article' || projection.kind === 'caseStudy')) {
    const metadata = projection.kind === 'article' ? articleMetadataFrom(projection) : { outline: [] }
    return {
      '@type': 'Article',
      headline: page.title,
      ...(page.description ? { description: page.description } : {}),
      inLanguage: projection.locale,
      dateModified: projection.updatedAt,
      ...(metadata.publishedAt ? { datePublished: metadata.publishedAt } : {}),
      ...(metadata.category ? { articleSection: metadata.category } : {}),
      ...(metadata.author ? {
        author: { '@type': metadata.authorType ?? 'Organization', name: metadata.author },
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
