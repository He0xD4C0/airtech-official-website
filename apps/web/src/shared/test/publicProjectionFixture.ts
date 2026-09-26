import type { PublicContentProjection, TiptapDocument } from '@airtek/contracts'

export function tiptapDocument(content: unknown[]): TiptapDocument {
  return { type: 'doc', content } as TiptapDocument
}

export function publicProjectionFixture(
  overrides: Partial<PublicContentProjection> = {},
): PublicContentProjection {
  return {
    schemaVersion: 2,
    id: '792406a9-2f19-425e-8508-78a205c0c764',
    kind: 'page',
    templateKey: 'home',
    locale: 'en',
    title: 'Published V2 page',
    summary: 'Published V2 summary.',
    slug: null,
    seo: { title: null, description: null, indexable: true, socialImage: null },
    body: null,
    composition: { blocks: [] },
    typeFields: { type: 'page' },
    isPlaceholder: false,
    publishedRevision: 2,
    updatedAt: '2026-09-01T08:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
    resolvedMedia: [],
    ...overrides,
  }
}
