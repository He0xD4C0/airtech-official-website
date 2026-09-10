import type {
  CmsContentKind,
  ContentBlock,
  ContentBlockKind,
  ContentDraftV2,
  ContentTemplateDefinition,
  ContentTypeFields,
} from '@airtek/contracts'

export function newDraftId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return '00000000-0000-4000-8000-000000000000'
}

export function defaultTypeFields(kind: CmsContentKind): ContentTypeFields {
  switch (kind) {
    case 'home':
      return { type: 'home' }
    case 'page':
      return { type: 'page' }
    case 'solution':
      return { type: 'solution', key: null }
    case 'technology':
      return { type: 'technology', key: null }
    case 'article':
      return {
        type: 'article',
        category: null,
        authorDisplayName: null,
        publicationAt: null,
        cover: null,
        featured: false,
      }
    case 'news':
      return {
        type: 'news',
        category: null,
        authorDisplayName: null,
        publicationAt: null,
        cover: null,
        featured: false,
      }
    case 'faq':
      return { type: 'faq', items: [] }
    case 'caseStudy':
      return { type: 'caseStudy', industry: null, location: null }
    case 'download':
      return { type: 'download', versionLabel: null, resourceType: null, versionNotes: null }
    case 'company':
      return { type: 'company' }
    case 'legal':
      return { type: 'legal', effectiveDate: null }
    case 'generalInformation':
      return {
        type: 'generalInformation',
        organizationName: null,
        brandLine: null,
        homePath: null,
        footerStatement: null,
        copyrightTemplate: null,
        contact: {
          email: null,
          phone: null,
          addressLines: [],
          locality: null,
          region: null,
          postalCode: null,
          countryCode: null,
        },
        socialLinks: [],
        defaultSeo: { title: null, description: null, indexable: false, socialImage: null },
        productCategories: [],
        navigationCta: null,
      }
    case 'navigation':
      return { type: 'navigation', items: [] }
    case 'footer':
      return { type: 'footer', columns: [], legalLinks: [] }
  }
}

/** Blocks that can be created without an existing media asset reference. */
export function defaultBlock(kind: ContentBlockKind, title: string | null): ContentBlock | null {
  switch (kind) {
    case 'hero':
      return {
        type: 'hero',
        id: newDraftId(),
        eyebrow: null,
        heading: title,
        lead: null,
        media: null,
        actions: [],
        variant: 'standard',
      }
    case 'body':
      return { type: 'body', id: newDraftId(), width: 'standard' }
    case 'faqCollection':
      return { type: 'faqCollection', id: newDraftId(), heading: null }
    case 'contactBlock':
      return { type: 'contactBlock', id: newDraftId(), heading: null, channels: [], action: null }
    case 'relationCollection':
      return {
        type: 'relationCollection',
        id: newDraftId(),
        heading: null,
        relationIds: [],
        presentation: 'cards',
      }
    case 'featureGrid':
      return { type: 'featureGrid', id: newDraftId(), heading: null, items: [] }
    case 'evidence':
      return { type: 'evidence', id: newDraftId(), heading: null, items: [] }
    case 'cta':
      return {
        type: 'cta',
        id: newDraftId(),
        eyebrow: null,
        heading: '',
        body: null,
        action: { label: '', target: { targetType: 'route', path: '/' } },
        variant: 'standard',
      }
    default:
      return null
  }
}

export function requiredBlocksForTemplate(
  template: ContentTemplateDefinition,
  title: string,
): ContentBlock[] {
  return template.requiredBlocks.flatMap((kind) => {
    const block = defaultBlock(kind, title || null)
    return block ? [block] : []
  })
}

export interface NewDraftInput {
  template: ContentTemplateDefinition
  title: string
  locale?: string
  slug?: string | null
  isPlaceholder?: boolean
}

export function draftFromTemplate(input: NewDraftInput): ContentDraftV2 {
  const { template, title } = input
  const locale = input.locale ?? 'en'
  return {
    schemaVersion: 2,
    kind: template.contentKind,
    locale,
    templateKey: template.key,
    title,
    slug: template.routable ? (input.slug?.trim() || null) : null,
    summary: null,
    isPlaceholder: input.isPlaceholder ?? true,
    typeFields: defaultTypeFields(template.contentKind),
    body: null,
    composition: { blocks: requiredBlocksForTemplate(template, title) },
    seo: { title: null, description: null, indexable: false, socialImage: null },
    relations: [],
    draftVersion: 1,
  }
}
