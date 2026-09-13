import { describe, expect, it, vi } from 'vitest'
import type { MediaAsset } from '@airtek/contracts'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'
import { getMediaAsset, listMediaAssetReferences, uploadMediaAsset } from './mediaApi'

setupAdminApiTestEnvironment()

const asset: MediaAsset = {
  id: '98000000-0000-4000-8000-000000000001',
  originalName: 'direct.png',
  mediaType: 'image/png',
  byteSize: 8,
  sha256: 'a'.repeat(64),
  publicUrl: '/api/public/v1/media/98000000-0000-4000-8000-000000000001',
  downloadUrl: '/api/public/v1/media/98000000-0000-4000-8000-000000000001/download',
  uploadedBy: 'admin@example.test',
  createdAt: '2026-09-13T00:00:00Z',
}

describe('direct media API', () => {
  it('uploads one file through the generated transport with the supplied idempotency key', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify(asset), {
        status: 201,
        headers: { 'Content-Type': 'application/json' },
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(uploadMediaAsset(
      new File(['png-data'], 'direct.png', { type: 'image/png' }),
      'upload-key-1',
    )).resolves.toEqual(asset)

    const [input, init] = fetchMock.mock.calls[0] ?? []
    expect(String(input)).toContain('/api/admin/v1/media/assets')
    expect(init?.method).toBe('POST')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBe('upload-key-1')
    expect(new Headers(init?.headers).has('Content-Type')).toBe(false)
    expect(init?.body).toBeInstanceOf(FormData)
  })

  it('loads asset detail and its content references from declared operations', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => new URL(String(input)).pathname.endsWith('/references')
      ? response({ items: [], nextCursor: null })
      : response(asset))
    vi.stubGlobal('fetch', fetchMock)

    await expect(getMediaAsset(asset.id)).resolves.toEqual(asset)
    await expect(listMediaAssetReferences(asset.id)).resolves.toEqual({ items: [], nextCursor: null })
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain(`/media/assets/${asset.id}`)
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain(`/media/assets/${asset.id}/references`)
  })

  it('preserves the stable idempotency conflict problem', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({
      type: 'https://api.airtekpower.example/problems/media_idempotency_conflict',
      title: 'Conflict',
      status: 409,
      detail: 'The idempotency key was already used for another file.',
      requestId: '98000000-0000-4000-8000-000000000099',
    }), { status: 409, headers: { 'Content-Type': 'application/problem+json' } })))

    await expect(uploadMediaAsset(
      new File(['other'], 'other.png', { type: 'image/png' }),
      'upload-key-1',
    )).rejects.toMatchObject({ status: 409, type: expect.stringContaining('media_idempotency_conflict') })
  })
})
