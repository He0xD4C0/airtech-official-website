import type {
  ContentDraftInput,
  ContentDraftV2,
  ContentEntry,
  ContentRecordV2,
  ContentRevisionV2,
  ContentTemplateKey,
  ContentTypeFields,
  GeneralInformation,
  GeneralInformationPayload,
  NewsDraftInput,
  NewsEntry,
  ProductCategoryPresentationInput,
  TiptapDocument,
} from '@airtek/contracts'
import { randomRequestId } from './adminApiTransport'

const templateByKind: Record<ContentDraftInput['kind'], ContentTemplateKey> = {
  home: 'home',
  solution: 'solutionDetail',
  technology: 'technologyDetail',
  article: 'articleDetail',
  news: 'newsDetail',
  faq: 'faqDetail',
  caseStudy: 'caseStudyDetail',
  download: 'downloadDetail',
  company: 'about',
  legal: 'legal',
  navigation: 'navigation',
  footer: 'footer',
}

export function legacyContent(record: ContentRecordV2): ContentEntry {
  return legacyContentDocument(record.id, record.status, record.draft, record.publishedRevision, record.updatedAt)
}

function legacyContentDocument(
  id: string,
  status: ContentEntry['status'],
  draft: ContentDraftV2,
  publishedRevision: number | null,
  updatedAt: string,
): ContentEntry {
  return {
    id,
    kind: legacyKind(draft),
    slug: draft.slug ?? '',
    locale: draft.locale,
    title: draft.title,
    summary: draft.summary ?? null,
    body: { schemaVersion: 1, doc: draft.body ?? { type: 'doc', content: [] } },
    seo: {
      title: draft.seo.title ?? null,
      description: draft.seo.description ?? null,
      canonicalPath: canonicalPath(draft),
      indexable: draft.seo.indexable,
    },
    status,
    currentRevision: draft.draftVersion,
    publishedRevision,
    scheduledFor: null,
    isPlaceholder: draft.isPlaceholder,
    updatedAt,
  }
}

export function genericDraft(
  input: ContentDraftInput,
  version: number,
  previous?: ContentDraftV2,
): ContentDraftV2 {
  const kind = input.kind
  const body = bodyAllowed(kind) ? tiptap(input.body.doc) : null
  return {
    ...(previous ?? {}),
    schemaVersion: 2,
    kind,
    locale: input.locale ?? 'en',
    templateKey: previous?.templateKey ?? templateByKind[kind],
    title: input.title,
    slug: configurationKind(kind) ? null : input.slug,
    summary: input.summary ?? null,
    isPlaceholder: input.isPlaceholder ?? false,
    typeFields: previous?.typeFields ?? defaultTypeFields(kind, input.body.doc),
    body,
    composition: previous?.composition ?? defaultComposition(kind, input.title, body !== null),
    seo: {
      title: input.seo?.title ?? null,
      description: input.seo?.description ?? null,
      indexable: input.seo?.indexable ?? false,
      socialImage: previous?.seo.socialImage ?? null,
    },
    relations: previous?.relations ?? [],
    draftVersion: version,
  }
}

export function newsDraft(
  input: NewsDraftInput,
  version: number,
  previous?: ContentDraftV2,
): ContentDraftV2 {
  const base = genericDraft({ ...input.content, kind: 'news' }, version, previous)
  return {
    ...base,
    kind: 'news',
    templateKey: 'newsDetail',
    typeFields: {
      type: 'news',
      category: input.category || null,
      authorDisplayName: input.authorDisplayName ?? null,
      publicationAt: input.publishedAt ?? null,
      cover: previous?.typeFields.type === 'news' ? previous.typeFields.cover ?? null : null,
      featured: input.featured ?? false,
    },
  }
}

export function legacyNews(record: ContentRecordV2): NewsEntry {
  return legacyNewsRevision(record.id, record.status, record.draft, record.publishedRevision, record.updatedAt)
}

export function legacyNewsRevisionFrom(value: ContentRevisionV2): NewsEntry {
  return legacyNewsRevision(
    value.contentId,
    value.kind === 'publish' ? 'published' : 'draft',
    { ...value.document, draftVersion: value.revision },
    value.kind === 'publish' ? value.revision : null,
    value.createdAt,
  )
}

function legacyNewsRevision(
  id: string,
  status: ContentEntry['status'],
  draft: ContentDraftV2,
  publishedRevision: number | null,
  updatedAt: string,
): NewsEntry {
  const fields = draft.typeFields.type === 'news' ? draft.typeFields : null
  return {
    content: legacyContentDocument(id, status, draft, publishedRevision, updatedAt),
    category: fields?.category ?? '',
    authorDisplayName: fields?.authorDisplayName ?? null,
    coverMediaId: fields?.cover?.asset.assetId ?? null,
    publishedAt: fields?.publicationAt ?? null,
    featured: fields?.featured ?? false,
    dataClass: draft.isPlaceholder ? 'developmentFixture' : 'editorial',
  }
}

export function generalInformationDraft(
  payload: GeneralInformationPayload,
  isPlaceholder: boolean,
  version: number,
  previous?: ContentDraftV2,
): ContentDraftV2 {
  const organization = record(payload.organization)
  const defaultSeo = record(payload.defaultSeo)
  const navigationCta = payload.navigationCta
  const categories = (payload.productCategories ?? []) as ProductCategoryPresentationInput[]
  return {
    schemaVersion: 2,
    kind: 'generalInformation',
    locale: 'en',
    templateKey: 'generalInformation',
    title: 'General Information',
    slug: null,
    summary: null,
    isPlaceholder,
    typeFields: {
      type: 'generalInformation',
      organizationName: payload.brandName || stringValue(organization.name),
      brandLine: payload.brandLine,
      homePath: payload.homePath || null,
      footerStatement: payload.footerStatement,
      copyrightTemplate: payload.copyrightText,
      contact: {
        email: stringValue(organization.salesEmail),
        phone: stringValue(organization.phone),
        addressLines: stringValue(organization.address) ? [stringValue(organization.address)!] : [],
        locality: null,
        region: null,
        postalCode: null,
        countryCode: null,
      },
      socialLinks: legacySocialLinks(organization.socialLinks),
      defaultSeo: {
        title: stringValue(defaultSeo.title),
        description: stringValue(defaultSeo.description),
        indexable: false,
        socialImage: null,
      },
      productCategories: categories,
      navigationCta: navigationCta?.label && navigationCta.href
        ? { label: navigationCta.label, target: linkTarget(navigationCta.href) }
        : null,
    },
    body: null,
    composition: previous?.composition ?? { blocks: [] },
    seo: previous?.seo ?? { title: null, description: null, indexable: false, socialImage: null },
    relations: previous?.relations ?? [],
    draftVersion: version,
  }
}

export function legacyGeneralInformation(recordValue: ContentRecordV2): GeneralInformation {
  return legacyGeneralInformationDocument(
    recordValue.id,
    recordValue.status,
    recordValue.draft,
    recordValue.publishedRevision,
    recordValue.updatedAt,
  )
}

export function legacyGeneralInformationRevisionFrom(value: ContentRevisionV2): GeneralInformation {
  return legacyGeneralInformationDocument(
    value.contentId,
    value.kind === 'publish' ? 'published' : 'draft',
    { ...value.document, draftVersion: value.revision },
    value.kind === 'publish' ? value.revision : null,
    value.createdAt,
  )
}

function legacyGeneralInformationDocument(
  id: string,
  status: GeneralInformation['status'],
  draft: ContentDraftV2,
  publishedRevision: number | null,
  updatedAt: string,
): GeneralInformation {
  const fields = draft.typeFields.type === 'generalInformation' ? draft.typeFields : null
  const address = fields?.contact.addressLines.join(', ') || null
  return {
    id,
    locale: draft.locale,
    status,
    currentRevision: draft.draftVersion,
    publishedRevision,
    payload: {
      brandName: fields?.organizationName ?? '',
      brandLine: fields?.brandLine ?? null,
      homePath: fields?.homePath ?? '',
      footerStatement: fields?.footerStatement ?? null,
      copyrightText: fields?.copyrightTemplate ?? null,
      defaultSeo: fields?.defaultSeo ?? {},
      organization: {
        name: fields?.organizationName ?? '',
        salesEmail: fields?.contact.email ?? null,
        phone: fields?.contact.phone ?? null,
        address,
        socialLinks: (fields?.socialLinks ?? []).map((item) => ({ label: item.service, url: item.url })),
      },
      productCategories: fields?.productCategories ?? [],
      navigationCta: fields?.navigationCta
        ? { label: fields.navigationCta.label, href: targetHref(fields.navigationCta.target) }
        : null,
    },
    isPlaceholder: draft.isPlaceholder,
    updatedAt,
  }
}

function defaultTypeFields(kind: ContentDraftInput['kind'], doc: unknown): ContentTypeFields {
  const attrs = record(record(doc).attrs)
  switch (kind) {
    case 'home': return { type: 'home' }
    case 'solution': return { type: 'solution', key: null }
    case 'technology': return { type: 'technology', key: null }
    case 'article': return { type: 'article', category: stringValue(attrs.category), authorDisplayName: stringValue(attrs.author), publicationAt: stringValue(attrs.publishedAt), cover: null, featured: false }
    case 'news': return { type: 'news', category: null, authorDisplayName: null, publicationAt: null, cover: null, featured: false }
    case 'faq': return { type: 'faq', items: [] }
    case 'caseStudy': return { type: 'caseStudy', industry: null, location: null }
    case 'download': return { type: 'download', versionLabel: stringValue(attrs.version), resourceType: stringValue(attrs.resourceType), versionNotes: stringValue(attrs.fileDescription) }
    case 'company': return { type: 'company' }
    case 'legal': return { type: 'legal', effectiveDate: null }
    case 'navigation': return { type: 'navigation', items: [] }
    case 'footer': return { type: 'footer', columns: [], legalLinks: [] }
  }
}

function defaultComposition(kind: ContentDraftInput['kind'], title: string, hasBody: boolean): ContentDraftV2['composition'] {
  if (configurationKind(kind)) return { blocks: [] }
  const blocks: ContentDraftV2['composition']['blocks'] = [{
    type: 'hero', id: randomRequestId(), heading: title || null, eyebrow: null,
    lead: null, media: null, actions: [], variant: 'standard',
  }]
  if (kind === 'faq') blocks.push({ type: 'faqCollection', id: randomRequestId(), heading: null })
  else if (hasBody) blocks.push({ type: 'body', id: randomRequestId(), width: 'standard' })
  return { blocks }
}

function tiptap(value: unknown): TiptapDocument {
  const source = record(value)
  return { type: 'doc', content: Array.isArray(source.content) ? source.content : [] } as TiptapDocument
}

function legacyKind(draft: ContentDraftV2): ContentEntry['kind'] {
  return draft.kind === 'page' || draft.kind === 'generalInformation' ? 'company' : draft.kind
}

function bodyAllowed(kind: ContentDraftInput['kind']): boolean {
  return !['faq', 'navigation', 'footer'].includes(kind)
}

function configurationKind(kind: ContentDraftInput['kind']): boolean {
  return kind === 'navigation' || kind === 'footer'
}

function canonicalPath(draft: ContentDraftV2): string | null {
  if (!draft.slug) return null
  if (draft.kind === 'news') return `/en/resources/news/${draft.slug}`
  return draft.kind === 'home' ? '/en' : `/en/${draft.slug}`
}

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null
}

function legacySocialLinks(value: unknown): Array<{ service: string; url: string }> {
  return Array.isArray(value) ? value.flatMap((item) => {
    const link = record(item)
    const service = stringValue(link.label)
    const url = stringValue(link.href) ?? stringValue(link.url)
    return service && url ? [{ service, url }] : []
  }) : []
}

function linkTarget(href: string): { targetType: 'route'; path: string } | { targetType: 'external'; url: string } {
  return href.startsWith('/') ? { targetType: 'route', path: href } : { targetType: 'external', url: href }
}

function targetHref(target: { targetType: string; path?: string; url?: string }): string {
  return target.targetType === 'route' ? target.path ?? '' : target.url ?? ''
}
