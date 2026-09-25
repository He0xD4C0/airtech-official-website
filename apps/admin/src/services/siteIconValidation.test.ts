import { describe, expect, it } from 'vitest'
import { siteIconIssue } from './siteIconValidation'

describe('site icon selection validation', () => {
  it.each([
    ['image/svg+xml', 512, 512, '格式不支持'],
    ['image/png', null, null, '缺少尺寸信息'],
    ['image/jpeg', 1024, 512, '必须为正方形'],
    ['image/webp', 256, 256, '至少需要 512 × 512'],
  ])('blocks incompatible library assets', (mediaType, originalWidth, originalHeight, issue) => {
    expect(siteIconIssue({ mediaType, originalWidth, originalHeight })).toBe(issue)
  })

  it('allows a supported square asset at the minimum size', () => {
    expect(siteIconIssue({
      mediaType: 'image/png', originalWidth: 512, originalHeight: 512,
    })).toBeUndefined()
  })
})
