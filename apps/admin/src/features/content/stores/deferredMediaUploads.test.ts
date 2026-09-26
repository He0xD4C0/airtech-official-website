import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { uploadMediaAsset } from '@/features/media'
import { PENDING_MEDIA_PREFIX, useDeferredMediaUploads } from '@/features/content/stores/deferredMediaUploads'

vi.mock('@/features/media/services/mediaApi', () => ({ uploadMediaAsset: vi.fn() }))

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.stubGlobal('URL', {
    createObjectURL: vi.fn(() => 'blob:pending'),
    revokeObjectURL: vi.fn(),
  })
})

afterEach(() => vi.unstubAllGlobals())

describe('deferred media uploads', () => {
  it('keeps files local until the explicit upload phase', async () => {
    const store = useDeferredMediaUploads()
    const id = store.register(new File(['image'], 'hero.png', { type: 'image/png' }))
    expect(id.startsWith(PENDING_MEDIA_PREFIX)).toBe(true)
    expect(uploadMediaAsset).not.toHaveBeenCalled()
    vi.mocked(uploadMediaAsset).mockResolvedValue({ id: 'asset-1' } as never)
    const result = await store.uploadValue({ media: { assetId: id } })
    expect(result).toEqual({ value: { media: { assetId: 'asset-1' } }, complete: true })
    expect(uploadMediaAsset).toHaveBeenCalledTimes(1)
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:pending')
  })

  it('materializes successful uploads while retaining a failed local file for retry', async () => {
    const store = useDeferredMediaUploads()
    const first = store.register(new File(['a'], 'a.png'))
    const second = store.register(new File(['b'], 'b.png'))
    vi.mocked(uploadMediaAsset)
      .mockResolvedValueOnce({ id: 'asset-a' } as never)
      .mockRejectedValueOnce(new Error('offline'))
    const result = await store.uploadValue([{ assetId: first }, { assetId: second }])
    expect(result.complete).toBe(false)
    expect(result.value).toEqual([{ assetId: 'asset-a' }, { assetId: second }])
    expect(store.entries[second]).toBeDefined()
    expect(store.entries[first]).toBeUndefined()
  })
})
