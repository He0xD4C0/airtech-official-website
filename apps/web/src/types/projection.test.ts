import { describe, expect, it, vi } from 'vitest'
import { downloadAssetHref, mediaAssetHref, type ResolvedMedia } from './projection'

const assetId = '11111111-1111-4111-8111-111111111111'
const resolved: ResolvedMedia[] = [{
  assetId,
  publicUrl: `/api/public/v1/media/${assetId}`,
  previewUrl: `https://media.example.test/${assetId}.preview.webp`,
  downloadUrl: `/api/public/v1/media/${assetId}/download`,
  mediaType: 'image/png', byteSize: 200, originalName: 'source.png',
  originalWidth: 1200, originalHeight: 800, previewWidth: 1200, previewHeight: 800,
  previewByteSize: 100,
}]

describe('resolved public media', () => {
  it('uses only hrefs supplied by the public projection', () => {
    const media = { asset: { assetId }, altText: 'Fan', decorative: false }
    expect(mediaAssetHref(media, resolved)).toBe(`https://media.example.test/${assetId}.preview.webp`)
    expect(mediaAssetHref(media, resolved, 'original')).toBe(`http://localhost:8080/api/public/v1/media/${assetId}`)
    expect(downloadAssetHref(media.asset, resolved)).toBe(`http://localhost:8080/api/public/v1/media/${assetId}/download`)
  })

  it('fails closed and emits a structured diagnostic when mapping is missing', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const media = { asset: { assetId }, altText: null, decorative: true }

    expect(mediaAssetHref(media, [])).toBeUndefined()
    expect(error).toHaveBeenCalledWith(
      '[public-media-resolution] Published media mapping is unavailable.',
      { assetId, use: 'inline' },
    )
  })
})
