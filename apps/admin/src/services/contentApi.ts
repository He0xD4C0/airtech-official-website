import type {
  CmsDraftPage,
  CmsPrivateDraft,
  CmsPublishedContent,
  CmsPublishedPage,
  CmsPublishResult,
  CmsReviewPage,
  CmsSubmitResult,
  ContentDraftV2,
  ContentTemplateDefinition,
  MediaAssetPage,
  Product,
} from '@airtek/contracts'
import { adminContractClient, draftEtag } from './adminApiTransport'

export interface ContentListQuery {
  q?: string
  cursor?: string
  limit?: number
}

export type ProductSearchQuery = ContentListQuery
export interface ProductSearchPage { items: Product[]; nextCursor: string | null }
export type MediaAssetQuery = ContentListQuery
export interface DraftResult { draft: CmsPrivateDraft; etag: string }

function query(value: ContentListQuery): { q?: string; cursor?: string; limit?: number } {
  return {
    ...(value.q?.trim() ? { q: value.q.trim() } : {}),
    ...(value.cursor ? { cursor: value.cursor } : {}),
    ...(value.limit === undefined ? {} : { limit: value.limit }),
  }
}

function draftResult(draft: CmsPrivateDraft, etag: string | null): DraftResult {
  return { draft, etag: etag ?? draftEtag(draft.draftVersion) }
}

export const contentApi = {
  async listDrafts(value: ContentListQuery = {}): Promise<CmsDraftPage> {
    return (await adminContractClient.get('/api/admin/v1/content-drafts', {
      parameters: { query: query(value) },
    })).data
  },

  async listTemplates(): Promise<ContentTemplateDefinition[]> {
    return (await adminContractClient.get('/api/admin/v1/content-drafts/templates')).data.items
  },

  async getDraft(draftId: string): Promise<DraftResult> {
    const result = await adminContractClient.get('/api/admin/v1/content-drafts/{draftId}', {
      parameters: { path: { draftId } },
    })
    return draftResult(result.data, result.etag)
  },

  async createDraft(document: ContentDraftV2): Promise<DraftResult> {
    const result = await adminContractClient.post('/api/admin/v1/content-drafts', { body: document })
    return draftResult(result.data, result.etag)
  },

  async saveDraft(draftId: string, document: ContentDraftV2): Promise<DraftResult> {
    const result = await adminContractClient.patch('/api/admin/v1/content-drafts/{draftId}', {
      parameters: {
        path: { draftId },
        header: { 'If-Match': draftEtag(document.draftVersion) },
      },
      body: document,
    })
    return draftResult(result.data, result.etag)
  },

  async setShares(draftId: string, userIds: string[]): Promise<DraftResult> {
    const result = await adminContractClient.put('/api/admin/v1/content-drafts/{draftId}/shares', {
      parameters: { path: { draftId } },
      body: { userIds },
    })
    return draftResult(result.data, result.etag)
  },

  async claimDraft(draftId: string): Promise<DraftResult> {
    const result = await adminContractClient.post('/api/admin/v1/content-drafts/{draftId}/claim', {
      parameters: { path: { draftId } },
    })
    return draftResult(result.data, result.etag)
  },

  async submit(draftId: string, draftVersion: number): Promise<CmsSubmitResult> {
    return (await adminContractClient.post('/api/admin/v1/content-drafts/{draftId}/submit', {
      parameters: {
        path: { draftId },
        header: { 'If-Match': draftEtag(draftVersion) },
      },
    })).data
  },

  async withdraw(draftId: string): Promise<DraftResult> {
    const result = await adminContractClient.post('/api/admin/v1/content-drafts/{draftId}/withdraw', {
      parameters: { path: { draftId } },
    })
    return draftResult(result.data, result.etag)
  },

  async listReviews(value: ContentListQuery = {}): Promise<CmsReviewPage> {
    return (await adminContractClient.get('/api/admin/v1/content-reviews', {
      parameters: { query: query(value) },
    })).data
  },

  async approve(draftId: string): Promise<CmsPublishResult> {
    return (await adminContractClient.post('/api/admin/v1/content-reviews/{draftId}/approve', {
      parameters: { path: { draftId } },
    })).data
  },

  async reject(draftId: string, reason: string): Promise<DraftResult> {
    const result = await adminContractClient.post('/api/admin/v1/content-reviews/{draftId}/reject', {
      parameters: { path: { draftId } }, body: { reason },
    })
    return draftResult(result.data, result.etag)
  },

  async listPublished(value: ContentListQuery = {}): Promise<CmsPublishedPage> {
    return (await adminContractClient.get('/api/admin/v1/published-content', {
      parameters: { query: query(value) },
    })).data
  },

  async getPublished(contentId: string): Promise<CmsPublishedContent> {
    return (await adminContractClient.get('/api/admin/v1/published-content/{contentId}', {
      parameters: { path: { contentId } },
    })).data
  },

  async copyPublished(contentId: string): Promise<DraftResult> {
    const result = await adminContractClient.post('/api/admin/v1/published-content/{contentId}/drafts', {
      parameters: { path: { contentId } },
    })
    return draftResult(result.data, result.etag)
  },

  async listMediaAssets(value: MediaAssetQuery = {}): Promise<MediaAssetPage> {
    return (await adminContractClient.get('/api/admin/v1/media/assets', {
      parameters: { query: query(value) },
    })).data
  },

  async searchProducts(value: ProductSearchQuery = {}): Promise<ProductSearchPage> {
    const result = await adminContractClient.get('/api/admin/v1/products', {
      parameters: { query: query(value) },
    })
    return { items: result.data.items, nextCursor: result.data.nextCursor }
  },
}
