import { describe, expect, it } from 'vitest'
import { siteIconMetadataIssue } from '@/features/content/services/siteIconAsset'

describe('site icon asset constraints', () => {
  it('accepts only supported square images at least 512 pixels wide', () => {
    expect(siteIconMetadataIssue({ mediaType: 'image/png', width: 512, height: 512 })).toBeNull()
    expect(siteIconMetadataIssue({ mediaType: 'image/webp', width: 1024, height: 1024 })).toBeNull()
    expect(siteIconMetadataIssue({ mediaType: 'application/pdf', width: 512, height: 512 })).toMatch(/PNG/)
    expect(siteIconMetadataIssue({ mediaType: 'image/png', width: 640, height: 512 })).toMatch(/正方形/)
    expect(siteIconMetadataIssue({ mediaType: 'image/jpeg', width: 256, height: 256 })).toMatch(/512/)
    expect(siteIconMetadataIssue({ mediaType: 'image/png', width: null, height: null })).toMatch(/尺寸/)
  })
})
