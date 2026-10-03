import { afterEach, beforeEach, vi } from 'vitest'
import type { BackendProduct } from '@/shared/services/adminApiTypes'

export function product(id: string): BackendProduct {
  return {
    id,
    stableId: `stable-${id}`,
    model: null,
    slug: `record-${id.slice(0, 8)}`,
    locale: 'en',
    family: 'axial',
    subtype: null,
    motorTechnology: null,
    title: 'API product record',
    summary: null,
    status: 'draft',
    specifications: [],
    sourceFacts: [],
    performanceCurves: [],
    sourceSnapshotId: '20000000-0000-4000-8000-000000000001',
    sourceRevision: 'source-1',
    currentRevision: 4,
    publishedRevision: null,
    indexable: false,
    seo: { title: null, description: null, canonicalPath: null, indexable: false },
    sortOrder: 0,
    relatedContentIds: [],
    mediaGallery: [],
    updatedAt: '2026-08-31T00:00:00Z',
  }
}

export function response(value: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(value), { status: 200, headers: { 'Content-Type': 'application/json', ...headers } })
}

export function setupAdminApiTestEnvironment(): void {
  beforeEach(() => {
    vi.stubGlobal('document', { cookie: '' })
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })
}
