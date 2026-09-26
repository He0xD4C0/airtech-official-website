import { adminAbsoluteUrl, adminContractClient } from '@/shared/services/adminApiTransport'
import type { BackendOperation, ProductImportAccepted } from '@/shared/services/adminApiTypes'

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
