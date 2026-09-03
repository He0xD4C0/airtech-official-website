import { adminContractClient, cursorQuery, randomRequestId } from './adminApiTransport'
import type {
  BackendSyncConflict,
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

  async listConflicts(pagination?: CursorPageRequest): Promise<CursorPage<BackendSyncConflict>> {
    const result = await adminContractClient.get('/api/admin/v1/feishu/conflicts', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async startSync(dryRun: boolean, mappingVersion = 'v1'): Promise<BackendSyncRun> {
    const result = await adminContractClient.post('/api/admin/v1/feishu/sync-runs', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: { dryRun, mappingVersion },
    })
    return result.data
  },
}
