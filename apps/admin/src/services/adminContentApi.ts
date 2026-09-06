import type { ContentDraftV2, ContentRecordV2 } from '@airtek/contracts'
import { findInCursorPages } from './cursorPagination'
import { adminContractClient, cursorQuery, draftEtag, randomRequestId } from './adminApiTransport'
import {
  generalInformationDraft,
  genericDraft,
  legacyContent,
  legacyGeneralInformation,
  legacyGeneralInformationRevisionFrom,
  legacyNews,
  legacyNewsRevisionFrom,
  newsDraft,
} from './contentV2Adapters'
import type {
  BackendContentEntry,
  BackendGeneralInformation,
  BackendNewsEntry,
  ContentDraftPayload,
  CursorPage,
  CursorPageRequest,
  GeneralInformationPayload,
  GeneralInformationRevision,
  NewsDraftPayload,
  NewsRevision,
} from './adminApiTypes'

const publishReason = 'Publish the current Admin CMS draft'
const manualSnapshotReason = 'Create a manual snapshot from the Admin CMS draft'

async function getRecord(id: string): Promise<ContentRecordV2> {
  const result = await adminContractClient.get('/api/admin/v1/content/{id}/draft', {
    parameters: { path: { id } },
  })
  return result.data
}

async function saveRecord(
  draft: ContentDraftV2,
  id?: string,
  version?: number,
): Promise<{ record: ContentRecordV2; etag: string }> {
  const result = id
    ? await adminContractClient.patch('/api/admin/v1/content/{id}/draft', {
        parameters: {
          path: { id },
          header: { 'Idempotency-Key': randomRequestId(), 'If-Match': draftEtag(version) },
        },
        body: draft,
      })
    : await adminContractClient.post('/api/admin/v1/content', {
        parameters: {
          header: { 'Idempotency-Key': randomRequestId(), 'If-Match': draftEtag(0, true) },
        },
        body: draft,
      })
  return { record: result.data, etag: result.etag ?? '' }
}

async function snapshotRecord(id: string, version: number, intent: 'manual' | 'publish', reason: string) {
  return adminContractClient.post('/api/admin/v1/content/{id}/snapshots', {
    parameters: {
      path: { id },
      header: { 'Idempotency-Key': randomRequestId(), 'If-Match': draftEtag(version) },
    },
    body: { intent, reason },
  })
}

async function restoreRecord(id: string, version: number, revision: number, reason: string) {
  return adminContractClient.post('/api/admin/v1/content/{id}/revisions/{revision}/restore', {
    parameters: {
      path: { id, revision },
      header: { 'Idempotency-Key': randomRequestId(), 'If-Match': draftEtag(version) },
    },
    body: { reason },
  })
}

async function findRecord(predicate: (record: ContentRecordV2) => boolean): Promise<ContentRecordV2 | undefined> {
  return (await findInCursorPages(
    async (pagination) => {
      const result = await adminContractClient.get('/api/admin/v1/content', {
        parameters: { query: cursorQuery(pagination) },
      })
      return result.data
    },
    predicate,
  )) ?? undefined
}

export const adminContentApi = {
  async listContent(pagination?: CursorPageRequest): Promise<CursorPage<BackendContentEntry>> {
    const result = await adminContractClient.get('/api/admin/v1/content', {
      parameters: { query: cursorQuery(pagination) },
    })
    return {
      ...result.data,
      items: result.data.items
        .filter((record) => !['news', 'generalInformation'].includes(record.draft.kind))
        .map(legacyContent),
    }
  },

  async findContent(idOrSlug: string): Promise<BackendContentEntry | undefined> {
    const record = await findRecord((item) => (
      item.draft.kind !== 'news'
      && item.draft.kind !== 'generalInformation'
      && (item.id === idOrSlug || item.draft.slug === idOrSlug)
    ))
    return record ? legacyContent(record) : undefined
  },

  async saveContent(payload: ContentDraftPayload, id?: string, version?: number) {
    const previous = id ? await getRecord(id) : undefined
    const result = await saveRecord(genericDraft(payload, version ?? 1, previous?.draft), id, version)
    return { entry: legacyContent(result.record), etag: result.etag }
  },

  async publishContent(id: string, version: number) {
    const result = await snapshotRecord(id, version, 'publish', publishReason)
    return { entry: legacyContent(result.data), etag: result.etag ?? '' }
  },

  async snapshotContent(id: string, version: number) {
    const result = await snapshotRecord(id, version, 'manual', manualSnapshotReason)
    return { entry: legacyContent(result.data), etag: result.etag ?? '' }
  },

  async listNews(pagination?: CursorPageRequest): Promise<CursorPage<BackendNewsEntry>> {
    const result = await adminContractClient.get('/api/admin/v1/content', {
      parameters: { query: cursorQuery(pagination) },
    })
    return { ...result.data, items: result.data.items.filter((item) => item.draft.kind === 'news').map(legacyNews) }
  },

  async getNews(id: string): Promise<BackendNewsEntry> {
    return legacyNews(await getRecord(id))
  },

  async saveNews(payload: NewsDraftPayload, id?: string, version?: number) {
    const previous = id ? await getRecord(id) : undefined
    const result = await saveRecord(newsDraft(payload, version ?? 1, previous?.draft), id, version)
    return { entry: legacyNews(result.record), etag: result.etag }
  },

  async publishNews(id: string, version: number) {
    const result = await snapshotRecord(id, version, 'publish', publishReason)
    return { entry: legacyNews(result.data), etag: result.etag ?? '' }
  },

  async snapshotNews(id: string, version: number) {
    const result = await snapshotRecord(id, version, 'manual', manualSnapshotReason)
    return { entry: legacyNews(result.data), etag: result.etag ?? '' }
  },

  async rollbackNews(id: string, version: number, targetRevision: number, reason: string) {
    const result = await restoreRecord(id, version, targetRevision, reason)
    return { entry: legacyNews(result.data), etag: result.etag ?? '' }
  },

  async listNewsRevisions(id: string): Promise<NewsRevision[]> {
    const result = await adminContractClient.get('/api/admin/v1/content/{id}/revisions', {
      parameters: { path: { id }, query: { limit: 100 } },
    })
    return result.data.items.map(legacyNewsRevisionFrom)
  },

  async getGeneralInformation(): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const record = await findRecord((item) => item.draft.kind === 'generalInformation')
    if (!record) throw new Error('General Information has not been created.')
    return { entry: legacyGeneralInformation(record), etag: draftEtag(record.draft.draftVersion) }
  },

  async saveGeneralInformation(
    payload: { locale: 'en'; payload: GeneralInformationPayload; isPlaceholder: boolean },
    id?: string,
    version?: number,
  ) {
    const previous = id ? await getRecord(id) : undefined
    const draft = generalInformationDraft(payload.payload, payload.isPlaceholder, version ?? 1, previous?.draft)
    const result = await saveRecord(draft, id, version)
    return { entry: legacyGeneralInformation(result.record), etag: result.etag }
  },

  async publishGeneralInformation(id: string, version: number) {
    const result = await snapshotRecord(id, version, 'publish', publishReason)
    return { entry: legacyGeneralInformation(result.data), etag: result.etag ?? '' }
  },

  async rollbackGeneralInformation(id: string, version: number, targetRevision: number, reason: string) {
    const result = await restoreRecord(id, version, targetRevision, reason)
    return { entry: legacyGeneralInformation(result.data), etag: result.etag ?? '' }
  },

  async listGeneralInformationRevisions(id: string): Promise<GeneralInformationRevision[]> {
    const result = await adminContractClient.get('/api/admin/v1/content/{id}/revisions', {
      parameters: { path: { id }, query: { limit: 100 } },
    })
    return result.data.items.map(legacyGeneralInformationRevisionFrom)
  },
}
