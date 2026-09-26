import type { MediaAsset } from '@airtek/contracts'

export interface SiteIconImageMetadata {
  mediaType: string
  width: number | null
  height: number | null
}

export function siteIconMetadataIssue(metadata: SiteIconImageMetadata): string | null {
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(metadata.mediaType)) {
    return '站点图标必须是 PNG、JPEG 或 WebP 图片。'
  }
  if (metadata.width === null || metadata.height === null) {
    return '站点图标缺少可验证的像素尺寸。'
  }
  if (metadata.width !== metadata.height) return '站点图标必须为正方形。'
  if (metadata.width < 512) return '站点图标至少需要 512 × 512 像素。'
  return null
}

export function siteIconAssetIssue(asset: MediaAsset): string | null {
  return siteIconMetadataIssue({
    mediaType: asset.mediaType,
    width: asset.originalWidth,
    height: asset.originalHeight,
  })
}

export async function readImageMetadata(file: File): Promise<SiteIconImageMetadata> {
  if (typeof createImageBitmap === 'function') {
    const bitmap = await createImageBitmap(file)
    const metadata = { mediaType: file.type, width: bitmap.width, height: bitmap.height }
    bitmap.close()
    return metadata
  }
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const image = new Image()
    image.onload = () => {
      URL.revokeObjectURL(url)
      resolve({ mediaType: file.type, width: image.naturalWidth, height: image.naturalHeight })
    }
    image.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error('无法读取站点图标的像素尺寸。'))
    }
    image.src = url
  })
}
