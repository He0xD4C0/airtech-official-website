interface SiteIconCandidate {
  mediaType: string
  originalWidth: number | null
  originalHeight: number | null
}

export function siteIconIssue(option: SiteIconCandidate): string | undefined {
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(option.mediaType)) return '格式不支持'
  if (!option.originalWidth || !option.originalHeight) return '缺少尺寸信息'
  if (option.originalWidth !== option.originalHeight) return '必须为正方形'
  if (option.originalWidth < 512) return '至少需要 512 × 512'
  return undefined
}
