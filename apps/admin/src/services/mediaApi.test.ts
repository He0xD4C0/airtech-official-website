import { describe, expect, it, vi } from 'vitest'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'
import { reviewMediaAsset } from './mediaApi'

setupAdminApiTestEnvironment()

const asset = {
  id: '98000000-0000-4000-8000-000000000001',
  versionId: '98000000-0000-8000-8000-000000000002',
  originalName: 'review.png',
  mediaType: 'image/png',
  byteSize: 1,
  scanStatus: 'clean',
  accessLevel: 'public',
  createdAt: '2026-09-11T00:00:00Z',
}

describe('media review API', () => {
  it.each(['clean', 'quarantined'] as const)(
    'sends a trimmed reason for a %s decision',
    async (status) => {
      const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
        void input
        void init
        return response({ ...asset, scanStatus: status })
      })
      vi.stubGlobal('fetch', fetchMock)

      await reviewMediaAsset(asset.id, status, '  Verified by an administrator.  ')

      const [, init] = fetchMock.mock.calls[0] ?? []
      expect(JSON.parse(String(init?.body))).toEqual({
        status,
        reason: 'Verified by an administrator.',
      })
    },
  )

  it.each(['clean', 'quarantined'] as const)(
    'rejects a blank reason before sending a %s decision',
    async (status) => {
      const fetchMock = vi.fn()
      vi.stubGlobal('fetch', fetchMock)

      await expect(reviewMediaAsset(asset.id, status, ' \n\t ')).rejects.toThrow(
        '人工审核决定必须填写原因。',
      )
      expect(fetchMock).not.toHaveBeenCalled()
    },
  )
})
