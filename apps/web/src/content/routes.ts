import { render } from 'vike/abort'
import { articles, caseEntries, faqCategories, productFamilies, solutions, technologies } from './catalog'
import type { Breadcrumb, PublicPageModel, RfqType } from '@/types/content'

const homeCrumb: Breadcrumb = { label: 'Home', href: '/en' }

function page(model: Omit<PublicPageModel, 'metaTitle' | 'breadcrumbs'> & { metaTitle?: string; breadcrumbs?: Breadcrumb[] }): PublicPageModel {
  return {
    ...model,
    metaTitle: model.metaTitle ?? `${model.title} | AIRTEKPOWER`,
    breadcrumbs: model.breadcrumbs ?? [homeCrumb, { label: model.title }],
  }
}

function collectionPage(
  canonicalPath: string,
  title: string,
  description: string,
  eyebrow: string,
  collection: PublicPageModel['collection'],
  entries: PublicPageModel['entries'],
): PublicPageModel {
  return page({ kind: 'collection', canonicalPath, title, description, eyebrow, collection, entries, indexable: true })
}

const fixed: Record<string, PublicPageModel> = {
  '/en': page({
    kind: 'home',
    canonicalPath: '/en',
    title: 'Industrial airflow, made easier to specify',
    metaTitle: 'AIRTEKPOWER | Industrial Airflow Engineering',
    description: 'Explore fan forms, engineering topics and structured selection paths built around verified product data.',
    eyebrow: 'AIRTEKPOWER',
    breadcrumbs: [],
    indexable: true,
  }),
  '/en/products': page({
    kind: 'catalog',
    canonicalPath: '/en/products',
    title: 'Products',
    description: 'Discover products by fan form, motor technology and application requirements.',
    eyebrow: 'Product discovery',
    indexable: true,
  }),
  '/en/products/selector': page({
    kind: 'selector',
    canonicalPath: '/en/products/selector',
    title: 'Fan Selector',
    description: 'Capture a duty point and hard constraints before searching validated product records.',
    eyebrow: 'Selection workspace',
    indexable: true,
  }),
  '/en/solutions': collectionPage('/en/solutions', 'Solutions', 'Start with the system and application context, then connect to verified products and engineering resources.', 'Application paths', 'solutions', solutions),
  '/en/technology': collectionPage('/en/technology', 'Technology', 'Technical topics designed to clarify product data, operating conditions and system integration.', 'Engineering library', 'technology', technologies),
  '/en/resources/articles': collectionPage('/en/resources/articles', 'Technical articles', 'Evidence-led notes for product discovery, selection and specification.', 'Resources', 'articles', articles),
  '/en/resources/case-studies': collectionPage('/en/resources/case-studies', 'Case studies', 'Case records are published only after their configuration, evidence and permissions are approved.', 'Resources', 'cases', caseEntries),
  '/en/resources/downloads': page({
    kind: 'downloads', canonicalPath: '/en/resources/downloads', title: 'Downloads',
    description: 'Find approved product documents by type, revision and applicable model.', eyebrow: 'Controlled resources', indexable: true,
  }),
  '/en/resources/faqs': page({
    kind: 'faq', canonicalPath: '/en/resources/faqs', title: 'Frequently asked questions',
    description: 'Browse questions by product, selection, technical, application and support context.', eyebrow: 'Knowledge base', entries: faqCategories, indexable: true,
  }),
  '/en/company/about': page({
    kind: 'about', canonicalPath: '/en/company/about', title: 'About AIRTEKPOWER',
    description: 'Our mission, vision and values for intelligent, energy-efficient ventilation solutions.', eyebrow: 'Company', indexable: true,
  }),
  '/en/company/contact': page({
    kind: 'contact', canonicalPath: '/en/company/contact', title: 'Contact',
    description: 'Route a general question to AIRTEKPOWER or start a structured request for quote.', eyebrow: 'Company', indexable: true,
  }),
  '/en/request-a-quote': page({
    kind: 'rfq-router', canonicalPath: '/en/request-a-quote', title: 'Request a quote',
    description: 'Choose the inquiry path that best matches your product, selection, project or replacement context.', eyebrow: 'Start an inquiry', indexable: true,
  }),
  '/en/search': page({
    kind: 'search', canonicalPath: '/en/search', title: 'Search',
    description: 'Search AIRTEKPOWER product families, solutions and engineering resources.', eyebrow: 'Site search', indexable: false,
  }),
  '/en/privacy': page({
    kind: 'legal', canonicalPath: '/en/privacy', title: 'Privacy',
    description: 'A publication-ready privacy notice will be supplied before production launch.', eyebrow: 'Legal', indexable: false,
  }),
  '/en/terms': page({
    kind: 'legal', canonicalPath: '/en/terms', title: 'Terms',
    description: 'A publication-ready terms notice will be supplied before production launch.', eyebrow: 'Legal', indexable: false,
  }),
  '/en/cookie-settings': page({
    kind: 'legal', canonicalPath: '/en/cookie-settings', title: 'Cookie settings',
    description: 'Review and change optional analytics consent.', eyebrow: 'Privacy controls', indexable: false,
  }),
}

const rfqNames: Record<RfqType, string> = {
  product: 'Product RFQ',
  selection: 'Fan Selection RFQ',
  project: 'Project RFQ',
  replacement: 'Replacement RFQ',
}

function validContentSlug(value: string): boolean {
  return value !== 'index'
    && value.length <= 180
    && !value.startsWith('-')
    && !value.endsWith('-')
    && !value.includes('--')
    && /^[a-z0-9-]+$/.test(value)
}

function slugLabel(value: string): string {
  return value.split('-').map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : '').join(' ')
}

function unpublishedContentPage(
  canonicalPath: string,
  title: string,
  eyebrow: string,
  collection?: PublicPageModel['collection'],
): PublicPageModel {
  return page({
    kind: 'detail',
    canonicalPath,
    slug: canonicalPath.split('/').at(-1),
    collection,
    title,
    description: 'This route is reserved for a published CMS record.',
    eyebrow,
    indexable: false,
    dataState: 'placeholder',
    placeholderReason: 'No published content projection is available for this route.',
    requiresPublishedContent: true,
  })
}

export function resolvePublicRoute(pathname: string): PublicPageModel {
  const normalized = pathname.length > 1 ? pathname.replace(/\/$/, '') : pathname
  if (fixed[normalized]) return fixed[normalized]

  const familyMatch = normalized.match(/^\/en\/products\/([^/]+)$/)
  if (familyMatch) {
    const family = productFamilies.find((entry) => entry.slug === familyMatch[1])
    if (family) {
      return page({
        kind: 'catalog', canonicalPath: normalized, category: family.slug, title: family.name,
        description: family.description, eyebrow: 'Product family', indexable: true,
        breadcrumbs: [homeCrumb, { label: 'Products', href: '/en/products' }, { label: family.name }],
      })
    }
  }

  const productMatch = normalized.match(/^\/en\/products\/([^/]+)\/([^/]+)$/)
  if (productMatch) {
    const family = productFamilies.find((entry) => entry.slug === productMatch[1])
    if (family && validContentSlug(productMatch[2])) {
      return page({
        kind: 'product-detail', canonicalPath: normalized, category: family.slug, slug: productMatch[2],
        title: 'Product record unavailable', description: 'No published Product Master record is available for this route.',
        eyebrow: family.name, indexable: false, dataState: 'placeholder',
        placeholderReason: 'The published product projection could not be loaded; no model values have been inferred.',
        breadcrumbs: [homeCrumb, { label: 'Products', href: '/en/products' }, { label: family.name, href: `/en/products/${family.slug}` }, { label: 'Product record' }],
      })
    }
  }

  for (const [base, name, entries, collection] of [
    ['/en/solutions', 'Solutions', solutions, 'solutions'],
    ['/en/technology', 'Technology', technologies, 'technology'],
    ['/en/resources/articles', 'Technical articles', articles, 'articles'],
    ['/en/resources/case-studies', 'Case studies', caseEntries, 'cases'],
  ] as const) {
    const match = normalized.match(new RegExp(`^${base}/([^/]+)$`))
    const slug = match?.[1]
    if (slug && validContentSlug(slug)) {
      const entry = entries.find((candidate) => candidate.slug === slug)
      const held = collection === 'cases'
      return page({
        kind: 'detail', canonicalPath: normalized, slug, collection,
        title: entry?.title ?? slugLabel(slug),
        description: entry?.summary ?? 'This route is reserved for a published CMS record.',
        eyebrow: entry?.eyebrow ?? name,
        indexable: entry ? !held : false,
        dataState: entry ? undefined : 'placeholder',
        placeholderReason: entry ? undefined : 'No published content projection is available for this route.',
        requiresPublishedContent: !entry,
        breadcrumbs: [homeCrumb, { label: name, href: base }, { label: entry?.title ?? slugLabel(slug) }],
      })
    }
  }

  const faqMatch = normalized.match(/^\/en\/resources\/faqs\/([^/]+)$/)
  if (faqMatch) {
    const slug = faqMatch[1]
    if (validContentSlug(slug)) {
      const category = faqCategories.find((entry) => entry.slug === slug)
      const label = category?.title ?? slugLabel(slug)
      return page({
        kind: 'faq', canonicalPath: normalized, slug, title: `${label} FAQ`,
        description: category?.summary ?? 'This route is reserved for a published FAQ category.',
        eyebrow: 'Frequently asked questions', entries: faqCategories, indexable: Boolean(category),
        dataState: category ? undefined : 'placeholder',
        placeholderReason: category ? undefined : 'No published FAQ projection is available for this route.',
        requiresPublishedContent: !category,
        breadcrumbs: [homeCrumb, { label: 'FAQ', href: '/en/resources/faqs' }, { label }],
      })
    }
  }

  const downloadMatch = normalized.match(/^\/en\/resources\/downloads\/([^/]+)$/)
  if (downloadMatch && validContentSlug(downloadMatch[1])) {
    const slug = downloadMatch[1]
    const label = slug === 'download-record-preview' ? 'Download record preview' : slugLabel(slug)
    const model = unpublishedContentPage(normalized, label, 'Controlled resource', 'downloads')
    model.breadcrumbs = [homeCrumb, { label: 'Downloads', href: '/en/resources/downloads' }, { label }]
    if (slug === 'download-record-preview') {
      model.description = 'A controlled-resource template awaiting an approved file, revision and product relationship.'
      model.placeholderReason = 'No approved file, revision and product relationship are published for this record.'
    }
    return model
  }

  const rfqMatch = normalized.match(/^\/en\/request-a-quote\/(product|selection|project|replacement)$/)
  if (rfqMatch) {
    const rfqType = rfqMatch[1] as RfqType
    return page({
      kind: 'rfq-form', canonicalPath: normalized, rfqType, title: rfqNames[rfqType],
      description: 'Provide structured context for a commercial and engineering follow-up.', eyebrow: 'Request a quote', indexable: false,
      breadcrumbs: [homeCrumb, { label: 'Request a quote', href: '/en/request-a-quote' }, { label: rfqNames[rfqType] }],
    })
  }

  throw render(404, 'The requested public page does not exist.')
}
