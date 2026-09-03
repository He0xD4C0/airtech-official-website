import type { RouteProjectionResponse } from '@/lib/publicApiTypes'
import type {
  PageKind,
  PublicPageModel,
  PublicSiteBootstrap,
  RfqType,
} from '@/types/content'
import {
  breadcrumbsFrom,
  isRecord,
  pageSlots,
  primaryCtaFrom,
  relationshipEntries,
  safeText,
  sectionsFrom,
} from './publicProjectionParsing'
import { PublicPageDataError } from './publicPageDataTypes'

interface TemplateResolution {
  kind: PageKind
  collection?: PublicPageModel['collection']
}

function templateResolution(route: RouteProjectionResponse): TemplateResolution {
  const key = route.templateKey.toLowerCase()
  const compact = key.replaceAll('-', '')
  const entity = route.entityType.toLowerCase()
  const indexLike = /(?:index|list|collection|landing)$/u.test(compact)
  if (route.path === '/en/products') return { kind: 'catalog' }
  if (route.path === '/en/products/selector') return { kind: 'selector' }
  if (route.path === '/en/products/compare') return { kind: 'compare' }
  if (/^\/en\/products\/[^/]+$/u.test(route.path)) return { kind: 'catalog' }
  if (entity === 'product' || /^\/en\/products\/[^/]+\/[^/]+$/u.test(route.path)) return { kind: 'product-detail' }
  if (route.path === '/en/request-a-quote') return { kind: 'rfq-router' }
  if (/^\/en\/request-a-quote\/(?:product|selection|project|replacement)$/u.test(route.path)) return { kind: 'rfq-form' }
  if (route.path === '/en/search') return { kind: 'search' }
  if (route.path === '/en/company/about') return { kind: 'about' }
  if (route.path === '/en/company/contact') return { kind: 'contact' }
  if (route.path === '/en/resources/news') return { kind: 'news' }
  if (/^\/en\/resources\/news\/[^/]+$/u.test(route.path)) return { kind: 'news-detail' }
  if (route.path === '/en/resources/downloads') return { kind: 'downloads', collection: 'downloads' }
  if (/^\/en\/resources\/downloads\/[^/]+$/u.test(route.path)) return { kind: 'detail', collection: 'downloads' }
  if (key === 'home') return { kind: 'home' }
  if (compact.includes('productdetail')) return { kind: 'product-detail' }
  if (compact.includes('productfamily') || key === 'products' || compact.includes('productindex') || key === 'catalog') return { kind: 'catalog' }
  if (key.includes('selector')) return { kind: 'selector' }
  if (key.includes('compare')) return { kind: 'compare' }
  if (key.includes('news')) return { kind: indexLike || route.path === '/en/resources/news' ? 'news' : 'news-detail' }
  if (key.includes('faq')) return { kind: 'faq' }
  if (key.includes('download')) return { kind: 'downloads', collection: 'downloads' }
  if (key.includes('about')) return { kind: 'about' }
  if (key.includes('contact')) return { kind: 'contact' }
  if (key.includes('search')) return { kind: 'search' }
  if (key.includes('legal') || entity === 'legal') return { kind: 'legal' }
  if (key.includes('rfq')) return { kind: key.includes('form') ? 'rfq-form' : 'rfq-router' }

  for (const [token, collection] of [
    ['solution', 'solutions'],
    ['technology', 'technology'],
    ['article', 'articles'],
    ['case', 'cases'],
  ] as const) {
    if (key.includes(token) || entity.includes(token)) {
      const basePath = collection === 'solutions'
        ? '/en/solutions'
        : collection === 'technology'
          ? '/en/technology'
          : collection === 'articles'
            ? '/en/resources/articles'
            : '/en/resources/case-studies'
      return { kind: indexLike || route.path === basePath ? 'collection' : 'detail', collection }
    }
  }
  throw new PublicPageDataError(`The published route template "${route.templateKey}" is unsupported.`, 503)
}

function rfqType(path: string): RfqType | undefined {
  const match = path.match(/^\/en\/request-a-quote\/(product|selection|project|replacement)$/u)
  return match?.[1] as RfqType | undefined
}

export function pageFromRoute(route: RouteProjectionResponse, site: PublicSiteBootstrap): PublicPageModel {
  const resolved = templateResolution(route)
  const content = route.page
  const slots = pageSlots(content)
  const hero = isRecord(slots.hero) ? slots.hero : {}
  const title = safeText(hero.title, 300) ?? safeText(content?.title, 300)
  const description = safeText(hero.description, 1_000)
    ?? safeText(content?.seo.description, 1_000)
    ?? safeText(content?.summary, 1_000)
    ?? site.defaultSeo.description
  const externallyTitled = resolved.kind === 'product-detail' || resolved.kind === 'news-detail'
  if (!title && !externallyTitled) {
    throw new PublicPageDataError('The published page projection is missing its required title.', 503)
  }
  if (!content && resolved.kind !== 'product-detail' && resolved.kind !== 'news-detail') {
    throw new PublicPageDataError('The published route has no page projection.', 503)
  }
  const contentRevision = content?.publishedRevision
  const publishedRevision = route.publishedRevision ?? contentRevision
  if (!publishedRevision) throw new PublicPageDataError('The published route has no revision.', 503)
  const placeholder = content?.isPlaceholder === true
  const analyticsEntityId = route.entityId ?? content?.id
  const policyNoIndex = resolved.kind === 'compare'
    || resolved.kind === 'search'
    || resolved.kind === 'rfq-form'

  return {
    kind: resolved.kind,
    canonicalPath: route.path,
    title: title ?? '',
    metaTitle: safeText(content?.seo.title, 300) ?? title ?? site.defaultSeo.title ?? '',
    description: description ?? '',
    eyebrow: safeText(hero.eyebrow, 160) ?? safeText(slots.eyebrow, 160) ?? '',
    breadcrumbs: breadcrumbsFrom(content, title ?? ''),
    indexable: !policyNoIndex
      && route.indexable
      && route.dataClass !== 'developmentFixture'
      && !site.isPlaceholder
      && !placeholder
      && content?.seo.indexable !== false,
    collection: resolved.collection,
    slug: content?.slug === 'index' ? undefined : content?.slug,
    rfqType: resolved.kind === 'rfq-form' ? rfqType(route.path) : undefined,
    entries: relationshipEntries(content),
    dataState: placeholder ? 'placeholder' : 'published',
    placeholderReason: placeholder ? 'This published record is explicitly marked as placeholder content.' : undefined,
    publishedContent: content ?? undefined,
    primaryCta: primaryCtaFrom(content),
    sections: sectionsFrom(content),
    productFamilies: site.productFamilies,
    motorTechnologies: site.motorTechnologies,
    analyticsContext: analyticsEntityId ? {
      contentKind: route.entityType === 'product' ? 'product' : 'content',
      contentId: analyticsEntityId,
      publishedRevision,
    } : undefined,
    dataClass: route.dataClass,
  }
}
