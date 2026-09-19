import type {
  FeishuConnectionTest,
  FeishuConnectionStatus,
  FeishuSettings,
  FeishuSyncRunDetail,
  StagingRecordPage,
  SyncMapping,
  UpdateFeishuSettings,
} from '@airtek/contracts'
import { adminContractClient, cursorQuery, revisionEtag } from './adminApiTransport'
import type {
  BackendSyncRun,
  CursorPage,
  CursorPageRequest,
} from './adminApiTypes'

export const adminIntegrationApi = {
  async listSyncRuns(pagination?: CursorPageRequest): Promise<CursorPage<BackendSyncRun>> {
    const result = await adminContractClient.get('/api/admin/v1/feishu/sync-runs', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async connectionStatus(): Promise<FeishuConnectionStatus> {
    return (await adminContractClient.get('/api/admin/v1/feishu/connection-status', {})).data
  },

  async getFeishuSettings(): Promise<{ settings: FeishuSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/feishu/settings', {})
    if (!result.etag) throw new Error('Feishu settings response did not include an ETag.')
    return { settings: result.data, etag: result.etag }
  },

  async updateFeishuSettings(
    revision: number,
    request: UpdateFeishuSettings,
  ): Promise<{ settings: FeishuSettings; etag: string }> {
    const result = await adminContractClient.put('/api/admin/v1/feishu/settings', {
      parameters: { header: { 'If-Match': revisionEtag(revision) } },
      body: request,
    })
    if (!result.etag) throw new Error('Feishu settings response did not include an ETag.')
    return { settings: result.data, etag: result.etag }
  },

  async testFeishuConnection(): Promise<FeishuConnectionTest> {
    return (await adminContractClient.post('/api/admin/v1/feishu/connection-test', {})).data
  },

  async startFeishuSync(): Promise<BackendSyncRun> {
    return (await adminContractClient.post('/api/admin/v1/feishu/sync-runs', {})).data
  },

  async getFeishuSyncRun(id: string): Promise<FeishuSyncRunDetail> {
    return (await adminContractClient.get('/api/admin/v1/feishu/sync-runs/{id}', {
      parameters: { path: { id } },
    })).data
  },

  async listMappings(pagination?: CursorPageRequest): Promise<CursorPage<SyncMapping>> {
    return (await adminContractClient.get('/api/admin/v1/feishu/mappings', {
      parameters: { query: cursorQuery(pagination) },
    })).data
  },

  async listStaging(pagination?: CursorPageRequest): Promise<StagingRecordPage> {
    return (await adminContractClient.get('/api/admin/v1/feishu/staging', {
      parameters: { query: cursorQuery(pagination) },
    })).data
  },

}
