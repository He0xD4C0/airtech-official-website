import { adminAbsoluteUrl, adminContractClient, cursorQuery, operationKind, randomRequestId, revisionEtag } from './adminApiTransport'
import type {
  BackendAuditEvent,
  BackendOperation,
  CursorPage,
  CursorPageRequest,
  PlatformSettings,
  ProductImportAccepted,
  ProductImportResult,
  UpdatePlatformSettings,
} from './adminApiTypes'

async function waitForOperationByPolling(id: string, deadline: number): Promise<BackendOperation> {
  while (Date.now() < deadline) {
    const result = await adminContractClient.get('/api/admin/v1/operations/{id}', {
      parameters: { path: { id } },
    })
    const operation = result.data
    if (operation.status === 'completed' || operation.status === 'failed') return operation
    await new Promise((resolve) => window.setTimeout(resolve, 750))
  }
  throw new Error('The background operation did not finish before the local timeout.')
}

export async function waitForOperation(
  accepted: ProductImportAccepted,
  timeoutMilliseconds = 120_000,
): Promise<BackendOperation> {
  const deadline = Date.now() + timeoutMilliseconds
  if (typeof EventSource === 'undefined') {
    return waitForOperationByPolling(accepted.operationId, deadline)
  }
  return new Promise<BackendOperation>((resolve, reject) => {
    const source = new EventSource(adminAbsoluteUrl(accepted.eventsUrl), { withCredentials: true })
    const timeout = window.setTimeout(() => {
      source.close()
      reject(new Error('The background operation did not finish before the local timeout.'))
    }, timeoutMilliseconds)
    source.addEventListener('operation', (event) => {
      try {
        const operation = JSON.parse(event.data) as BackendOperation
        if (operation.id !== accepted.operationId) throw new Error('The operation stream returned a different operation.')
        if (operation.status === 'completed' || operation.status === 'failed') {
          window.clearTimeout(timeout)
          source.close()
          resolve(operation)
        }
      } catch (error) {
        window.clearTimeout(timeout)
        source.close()
        reject(error)
      }
    })
    source.addEventListener('error', () => {
      source.close()
      void waitForOperationByPolling(accepted.operationId, deadline).then(resolve, reject)
    }, { once: true })
  })
}

export function productImportResult(operation: BackendOperation): ProductImportResult {
  if (operation.status === 'failed') throw new Error('Product Master import failed. Review the operation audit record.')
  const value = operation.result
  if (!value || typeof value !== 'object' || !('import' in value)) {
    throw new Error('The completed operation did not contain a Product Master import report.')
  }
  return (value as { import: ProductImportResult }).import
}

export const adminOperationsApi = {
  async getSettings(): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/settings')
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async updateSettings(payload: UpdatePlatformSettings, revision: number): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await adminContractClient.patch('/api/admin/v1/settings', {
      parameters: { header: { 'If-Match': revisionEtag(revision) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },

  async listOperations(pagination?: CursorPageRequest): Promise<CursorPage<BackendOperation>> {
    const result = await adminContractClient.get('/api/admin/v1/operations', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async getOperation(id: string): Promise<BackendOperation> {
    const result = await adminContractClient.get('/api/admin/v1/operations/{id}', {
      parameters: { path: { id } },
    })
    return result.data
  },

  async createOperation(kind: string, reason: string, confirmation: string, otp: string): Promise<BackendOperation> {
    const result = await adminContractClient.post('/api/admin/v1/operations', {
      parameters: {
        header: { 'Idempotency-Key': randomRequestId(), 'X-TOTP-Code': otp },
      },
      body: { kind: operationKind(kind), reason, confirmation },
    })
    return result.data
  },

  async listAudit(pagination?: CursorPageRequest): Promise<CursorPage<BackendAuditEvent>> {
    const result = await adminContractClient.get('/api/admin/v1/audit', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
}
