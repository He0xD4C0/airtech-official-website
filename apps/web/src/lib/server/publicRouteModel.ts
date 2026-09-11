import type { RouteProjectionResponse } from '@/lib/publicApiTypes'
import type { PageKind, PublicPageModel, PublicSiteBootstrap, RfqType } from '@/types/content'
import { safeText, slugPattern } from './publicProjectionParsing'
import { PublicPageDataError } from './publicPageDataTypes'
import {
  breadcrumbsFromProjection,
  newsMetadataFrom,
  pageDescription,
  pageEyebrow,
  pageTitle,
  primaryCtaFromBlocks,
  relationshipEntriesFromProjection,
  sectionsFromBlocks,
} from './publicProjectionV2'

interface TemplateResolution {
  kind: PageKind
  collection?: PublicPageModel['collection']
}

const slugTemplates = new Set([
  'productFamily', 'solutionDetail', 'technologyDetail', 'articleDetail', 'newsDetail',
  'faqDetail', 'caseStudyDetail', 'downloadDetail', 'rfqForm', 'legal',
])

function templateResolution(templateKey: string): TemplateResolution {
  switch (templateKey) {
    case 'home': return { kind: 'home' }
    case 'productIndex':
    case 'productFamily': return { kind: 'catalog' }
    case 'productDetail': return { kind: 'product-detail' }
    case 'selector': return { kind: 'selector' }
    case 'compare': return { kind: 'compare' }
    case 'solutionIndex': return { kind: 'collection', collection: 'solutions' }
    case 'solutionDetail': return { kind: 'detail', collection: 'solutions' }
    case 'technologyIndex': return { kind: 'collection', collection: 'technology' }
    case 'technologyDetail': return { kind: 'detail', collection: 'technology' }
    case 'articleIndex': return { kind: 'collection', collection: 'articles' }
    case 'articleDetail': return { kind: 'detail', collection: 'articles' }
    case 'newsIndex': return { kind: 'news' }
    case 'newsDetail': return { kind: 'news-detail' }
    case 'faqIndex':
    case 'faqDetail': return { kind: 'faq' }
    case 'caseStudyIndex': return { kind: 'collection', collection: 'cases' }
    case 'caseStudyDetail': return { kind: 'detail', collection: 'cases' }
    case 'downloadIndex': return { kind: 'downloads', collection: 'downloads' }
    case 'downloadDetail': return { kind: 'detail', collection: 'downloads' }
    case 'about': return { kind: 'about' }
    case 'contact': return { kind: 'contact' }
    case 'rfqRouter': return { kind: 'rfq-router' }
    case 'rfqForm': return { kind: 'rfq-form' }
    case 'search': return { kind: 'search' }
    case 'legal': return { kind: 'legal' }
    default:
      throw new PublicPageDataError(`The published route template "${templateKey}" is unsupported.`, 503)
  }
}

function rfqType(path: string): RfqType | undefined {
  const value = path.match(/^\/en\/request-a-quote\/(product|selection|project|replacement)$/u)?.[1]
  if (value === 'product' || value === 'selection' || value === 'project' || value === 'replacement') {
    return value
  }
  return undefined
}

function validateProjectionSlug(route: RouteProjectionResponse): string | undefined {
  const projection = route.page
  if (!projection) return undefined
  const slug = projection.slug
  if (slug !== null && !slugPattern.test(slug)) {
    throw new PublicPageDataError('The published page projection has an invalid slug.', 503)
  }
  if (!slugTemplates.has(projection.templateKey)) return slug ?? undefined
  if (!slug || route.path.split('/').at(-1) !== slug) {
    throw new PublicPageDataError('The published detail route has no matching CMS V2 slug.', 503)
  }
  return slug
}

function productPageFromRoute(
  route: RouteProjectionResponse,
  site: PublicSiteBootstrap,
): PublicPageModel {
  if (route.templateKey !== 'productDetail'
    || route.page !== null
    || !route.entityId
    || !route.publishedRevision
    || !/^\/en\/products\/[a-z0-9]+(?:-[a-z0-9]+)*\/[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(route.path)) {
    throw new PublicPageDataError('The published product route projection is inconsistent.', 503)
  }
  return {
    kind: 'product-detail',
    canonicalPath: route.path,
    title: '',
    metaTitle: site.defaultSeo.title ?? '',
    description: site.defaultSeo.description ?? '',
    eyebrow: '',
    breadcrumbs: [],
    indexable: route.indexable && route.dataClass !== 'developmentFixture' && !site.isPlaceholder,
    productFamilies: site.productFamilies,
    motorTechnologies: site.motorTechnologies,
    analyticsContext: {
      contentKind: 'product',
      contentId: route.entityId,
      publishedRevision: route.publishedRevision,
    },
    dataClass: route.dataClass,
  }
}

export function pageFromRoute(
  route: RouteProjectionResponse,
  site: PublicSiteBootstrap,
): PublicPageModel {
  if (route.entityType === 'product') return productPageFromRoute(route, site)
  const projection = route.page
  if (!projection
    || route.entityType !== 'content'
    || !route.entityId
    || route.entityId !== projection.id
    || route.publishedRevision !== projection.publishedRevision
    || route.templateKey !== projection.templateKey
    || route.locale !== projection.locale) {
    throw new PublicPageDataError('The published content route projection is inconsistent.', 503)
  }
  const resolved = templateResolution(projection.templateKey)
  const title = pageTitle(projection)
  if (!title) {
    throw new PublicPageDataError('The published page projection is missing its required title.', 503)
  }
  const slug = validateProjectionSlug(route)
  const policyNoIndex = resolved.kind === 'compare'
    || resolved.kind === 'search'
    || resolved.kind === 'rfq-form'
  return {
    kind: resolved.kind,
    canonicalPath: route.path,
    title,
    metaTitle: safeText(projection.seo.title, 300) ?? title,
    description: pageDescription(projection) ?? site.defaultSeo.description ?? '',
    eyebrow: pageEyebrow(projection),
    breadcrumbs: breadcrumbsFromProjection(projection, route.path),
    indexable: !policyNoIndex
      && route.indexable
      && route.dataClass !== 'developmentFixture'
      && !site.isPlaceholder
      && !projection.isPlaceholder
      && projection.seo.indexable,
    collection: resolved.collection,
    slug,
    rfqType: resolved.kind === 'rfq-form' ? rfqType(route.path) : undefined,
    entries: relationshipEntriesFromProjection(projection),
    newsMetadata: newsMetadataFrom(projection),
    dataState: projection.isPlaceholder ? 'placeholder' : 'published',
    placeholderReason: projection.isPlaceholder
      ? 'This published record is explicitly marked as placeholder content.'
      : undefined,
    projection,
    blocks: projection.composition.blocks,
    primaryCta: primaryCtaFromBlocks(projection),
    sections: sectionsFromBlocks(projection),
    productFamilies: site.productFamilies,
    motorTechnologies: site.motorTechnologies,
    analyticsContext: {
      contentKind: projection.kind === 'news' ? 'news' : 'content',
      contentId: route.entityId,
      publishedRevision: projection.publishedRevision,
    },
    dataClass: route.dataClass,
  }
}
