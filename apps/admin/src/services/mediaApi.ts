import type { MediaAsset, MediaAssetReferencePage } from '@airtek/contracts'
import { adminContractClient, randomRequestId } from './adminApiTransport'

/** Uploads one supported image and returns its immediately public asset. */
export async function uploadMediaAsset(
  file: File,
  idempotencyKey = randomRequestId(),
): Promise<MediaAsset> {
  const form = new FormData()
  form.append('file', file, file.name)
  const result = await adminContractClient.post('/api/admin/v1/media/assets', {
    parameters: { header: { 'Idempotency-Key': idempotencyKey } },
    body: form,
  })
  return result.data
}

export async function getMediaAsset(id: string): Promise<MediaAsset> {
  const result = await adminContractClient.get('/api/admin/v1/media/assets/{id}', {
    parameters: { path: { id } },
  })
  return result.data
}

export async function listMediaAssetReferences(
  id: string,
  cursor?: string,
  limit = 50,
): Promise<MediaAssetReferencePage> {
  const result = await adminContractClient.get('/api/admin/v1/media/assets/{id}/references', {
    parameters: { path: { id }, query: { ...(cursor ? { cursor } : {}), limit } },
  })
  return result.data
}
