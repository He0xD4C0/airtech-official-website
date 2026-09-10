import type {
  ContentDiffV2,
  ContentDraftV2,
  ContentRecordV2,
  ContentRevisionV2,
  ContentTemplateDefinition,
  Product,
} from '@airtek/contracts'
import { adminContractClient, draftEtag, randomRequestId } from './adminApiTransport'

export type ContentSortField = 'updatedAt' | 'title' | 'kind'
export type SortDirection = 'asc' | 'desc'
export type ContentStatusFilter = 'draft' | 'published' | 'archived'
export type SnapshotIntent = 'manual' | 'publish'

export interface ContentListQuery {
  q?: string
  kinds?: string[]
  status?: ContentStatusFilter
  sort?: ContentSortField
  direction?: SortDirection
  cursor?: string
  limit?: number
}

export interface ContentListPage {
  items: ContentRecordV2[]
  nextCursor: string | null
  total: number
  counts: Record<string, number>
}

export type MediaScanStatus = 'pending' | 'clean' | 'quarantined' | 'failed'
export type MediaAccessLevel = 'public' | 'authenticated' | 'internal'

export interface MediaAssetSummary {
  id: string
  versionId: string
  originalName: string
  mediaType: string
  byteSize: number
  scanStatus: MediaScanStatus
  accessLevel: MediaAccessLevel
  createdAt: string
}

export interface MediaAssetQuery {
  q?: string
  scanStatus?: MediaScanStatus
  accessLevel?: MediaAccessLevel
  cursor?: string
  limit?: number
}

export interface MediaAssetPage {
  items: MediaAssetSummary[]
  nextCursor: string | null
}

export interface ProductSearchQuery {
  q?: string
  cursor?: string
  limit?: number
}

export interface ProductSearchPage {
  items: Product[]
  nextCursor: string | null
}

export interface DraftRecordResult {
  record: ContentRecordV2
  etag: string
}

function queryWithoutUndefined<T extends Record<string, unknown>>(value: T): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined && entry !== null && entry !== ''),
  )
}

function draftResults(record: ContentRecordV2, etag: string | null): DraftRecordResult {
  return { record, etag: etag ?? draftEtag(record.draft.draftVersion) }
}

export const contentApi = {
  async listContent(query: ContentListQuery = {}): Promise<ContentListPage> {
    const result = await adminContractClient.get('/api/admin/v1/content', {
      parameters: {
        query: queryWithoutUndefined({
          q: query.q?.trim() || undefined,
          kind: query.kinds?.length ? query.kinds.join(',') : undefined,
          status: query.status,
          sort: query.sort,
          direction: query.direction,
          cursor: query.cursor,
          limit: query.limit,
        }),
      },
    })
    return result.data as unknown as ContentListPage
  },

  async listTemplates(): Promise<ContentTemplateDefinition[]> {
    const result = await adminContractClient.get('/api/admin/v1/content/templates', {})
    return (result.data as unknown as { items: ContentTemplateDefinition[] }).items
  },

  async getContentRecord(id: string): Promise<DraftRecordResult> {
    const result = await adminContractClient.get('/api/admin/v1/content/{id}/draft', {
      parameters: { path: { id } },
    })
    return draftResults(result.data, result.etag)
  },

  async createContent(draft: ContentDraftV2): Promise<DraftRecordResult> {
    const result = await adminContractClient.post('/api/admin/v1/content', {
      parameters: {
        header: {
          'Idempotency-Key': randomRequestId(),
          'If-Match': draftEtag(0, true),
        },
      },
      body: draft,
    })
    return draftResults(result.data, result.etag)
  },

  async saveDraft(id: string, draft: ContentDraftV2): Promise<DraftRecordResult> {
    const result = await adminContractClient.patch('/api/admin/v1/content/{id}/draft', {
      parameters: {
        path: { id },
        header: {
          'Idempotency-Key': randomRequestId(),
          'If-Match': draftEtag(draft.draftVersion),
        },
      },
      body: draft,
    })
    return draftResults(result.data, result.etag)
  },

  async createSnapshot(
    id: string,
    draftVersion: number,
    intent: SnapshotIntent,
    reason: string,
  ): Promise<DraftRecordResult> {
    const result = await adminContractClient.post('/api/admin/v1/content/{id}/snapshots', {
      parameters: {
        path: { id },
        header: {
          'Idempotency-Key': randomRequestId(),
          'If-Match': draftEtag(draftVersion),
        },
      },
      body: { intent, reason },
    })
    return draftResults(result.data, result.etag)
  },

  async listRevisions(id: string, limit = 100): Promise<ContentRevisionV2[]> {
    const result = await adminContractClient.get('/api/admin/v1/content/{id}/revisions', {
      parameters: { path: { id }, query: { limit } },
    })
    return result.data.items
  },

  async diff(id: string, baseRevision: number, targetRevision?: number): Promise<ContentDiffV2> {
    const result = await adminContractClient.get('/api/admin/v1/content/{id}/diff', {
      parameters: {
        path: { id },
        query: { baseRevision, ...(targetRevision === undefined ? {} : { targetRevision }) },
      },
    })
    return result.data
  },

  async restoreRevision(
    id: string,
    draftVersion: number,
    revision: number,
    reason: string,
  ): Promise<DraftRecordResult> {
    const result = await adminContractClient.post(
      '/api/admin/v1/content/{id}/revisions/{revision}/restore',
      {
        parameters: {
          path: { id, revision },
          header: {
            'Idempotency-Key': randomRequestId(),
            'If-Match': draftEtag(draftVersion),
          },
        },
        body: { reason },
      },
    )
    return draftResults(result.data, result.etag)
  },

  async listMediaAssets(query: MediaAssetQuery = {}): Promise<MediaAssetPage> {
    const result = await adminContractClient.get('/api/admin/v1/media/assets', {
      parameters: {
        query: queryWithoutUndefined({
          q: query.q?.trim() || undefined,
          scanStatus: query.scanStatus,
          accessLevel: query.accessLevel,
          cursor: query.cursor,
          limit: query.limit,
        }),
      },
    })
    return result.data
  },

  async searchProducts(query: ProductSearchQuery = {}): Promise<ProductSearchPage> {
    const result = await adminContractClient.get('/api/admin/v1/products', {
      parameters: {
        query: queryWithoutUndefined({
          q: query.q?.trim() || undefined,
          cursor: query.cursor,
          limit: query.limit,
        }),
      },
    })
    return result.data as unknown as ProductSearchPage
  },
}
