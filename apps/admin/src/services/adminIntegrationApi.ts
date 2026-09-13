import type {
  FeishuConnectionStatus,
  ResolveSyncConflictRequest,
  StagingRecordPage,
  SyncConflict,
  SyncConflictPage,
  SyncMapping,
} from '@airtek/contracts'
import { adminContractClient, cursorQuery, randomRequestId, revisionEtag } from './adminApiTransport'
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

  async listConflicts(pagination?: CursorPageRequest): Promise<SyncConflictPage> {
    const result = await adminContractClient.get('/api/admin/v1/feishu/conflicts', {
      parameters: { query: { ...cursorQuery(pagination), openOnly: true } },
    })
    return result.data
  },

  async connectionStatus(): Promise<FeishuConnectionStatus> {
    return (await adminContractClient.get('/api/admin/v1/feishu/connection-status', {})).data
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

  async resolveConflict(
    id: string,
    revision: number,
    request: ResolveSyncConflictRequest,
  ): Promise<SyncConflict> {
    return (await adminContractClient.post('/api/admin/v1/feishu/conflicts/{id}/resolve', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
      body: request,
    })).data
  },

  async startSync(dryRun: boolean, mappingVersion = 'v1'): Promise<BackendSyncRun> {
    const result = await adminContractClient.post('/api/admin/v1/feishu/sync-runs', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: { dryRun, mappingVersion },
    })
    return result.data
  },
}
