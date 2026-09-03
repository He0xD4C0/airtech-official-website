import { findInCursorPages } from './cursorPagination'
import { adminContractClient, cursorQuery, randomRequestId, revisionEtag } from './adminApiTransport'
import type {
  BackendContentEntry,
  BackendGeneralInformation,
  BackendNewsEntry,
  ContentDraftPayload,
  ContentPreviewLink,
  CursorPage,
  CursorPageRequest,
  GeneralInformationPayload,
  GeneralInformationRevision,
  NewsDraftPayload,
  NewsRevision,
} from './adminApiTypes'

export const adminContentApi = {
  async listContent(pagination?: CursorPageRequest): Promise<CursorPage<BackendContentEntry>> {
    const result = await adminContractClient.get('/api/admin/v1/content', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  findContent: (idOrSlug: string) => findInCursorPages(
    async (pagination) => {
      const result = await adminContractClient.get('/api/admin/v1/content', {
        parameters: { query: cursorQuery(pagination) },
      })
      return result.data
    },
    (entry) => entry.id === idOrSlug || entry.slug === idOrSlug,
  ),

  async saveContent(payload: ContentDraftPayload, id?: string, revision?: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const result = id
      ? await adminContractClient.patch('/api/admin/v1/content/{id}', {
          parameters: {
            path: { id },
            header: {
              'Idempotency-Key': randomRequestId(),
              'If-Match': revisionEtag(revision),
            },
          },
          body: payload,
        })
      : await adminContractClient.post('/api/admin/v1/content', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async publishContent(id: string, revision: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/content/{id}/publish', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async createContentPreview(id: string, revision: number, expiresInSeconds = 600): Promise<ContentPreviewLink> {
    const result = await adminContractClient.post('/api/admin/v1/content/{id}/preview', {
      parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision) } },
      body: { revision, expiresInSeconds },
    })
    return result.data
  },

  async listNews(pagination?: CursorPageRequest): Promise<CursorPage<BackendNewsEntry>> {
    const result = await adminContractClient.get('/api/admin/v1/news', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async getNews(id: string): Promise<BackendNewsEntry> {
    const result = await adminContractClient.get('/api/admin/v1/news/{id}', { parameters: { path: { id } } })
    return result.data
  },

  async saveNews(payload: NewsDraftPayload, id?: string, revision?: number): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = id
      ? await adminContractClient.patch('/api/admin/v1/news/{id}', {
          parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
      : await adminContractClient.post('/api/admin/v1/news', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async publishNews(id: string, revision: number): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/news/{id}/publish', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async rollbackNews(id: string, revision: number, targetRevision: number, reason: string): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/news/{id}/rollback', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
      body: { revision: targetRevision, reason },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async listNewsRevisions(id: string): Promise<NewsRevision[]> {
    const result = await adminContractClient.get('/api/admin/v1/news/{id}/revisions', {
      parameters: { path: { id } },
    })
    return result.data.items
  },

  async getGeneralInformation(): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/general-information', { parameters: { query: { locale: 'en' } } })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async saveGeneralInformation(payload: { locale: 'en'; payload: GeneralInformationPayload; isPlaceholder: boolean }, id?: string, revision?: number): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = id
      ? await adminContractClient.patch('/api/admin/v1/general-information/{id}', {
          parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
      : await adminContractClient.post('/api/admin/v1/general-information', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async publishGeneralInformation(id: string, revision: number): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/general-information/{id}/publish', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async rollbackGeneralInformation(id: string, revision: number, targetRevision: number, reason: string): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await adminContractClient.post('/api/admin/v1/general-information/{id}/rollback', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
      body: { revision: targetRevision, reason },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },

  async listGeneralInformationRevisions(id: string): Promise<GeneralInformationRevision[]> {
    const result = await adminContractClient.get('/api/admin/v1/general-information/{id}/revisions', {
      parameters: { path: { id } },
    })
    return result.data.items
  },
}
