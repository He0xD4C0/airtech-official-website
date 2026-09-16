import type {
  LinkTargetReference,
  MediaUseReference,
  PublicContentProjection as GeneratedPublicContentProjection,
  ResolvedLinkTarget as GeneratedResolvedLinkTarget,
  ResolvedMedia as GeneratedResolvedMedia,
  ResolvedRelationCard as GeneratedResolvedRelationCard,
} from '@airtek/contracts'

export type ResolvedRelationCard = GeneratedResolvedRelationCard
export type ResolvedLinkTarget = GeneratedResolvedLinkTarget
export type ResolvedMedia = GeneratedResolvedMedia

/**
 * Native V2 public projection served by the platform API. The server has
 * already resolved relations and content links so the browser never needs
 * database access or UUID guessing.
 */
export type PublicContentProjection = GeneratedPublicContentProjection

function publicMediaHref(href: string): string {
  if (/^https?:\/\//u.test(href)) return href
  const apiBase = import.meta.env.VITE_PUBLIC_API_BASE_URL
    || 'http://localhost:8080/api/public/v1'
  return new URL(href, new URL(apiBase).origin).toString()
}

export function linkTargetHref(
  target: LinkTargetReference | undefined,
  resolvedLinks: ResolvedLinkTarget[],
): string | undefined {
  if (!target) return undefined
  if (target.targetType === 'route') return target.path || undefined
  if (target.targetType === 'external') return target.url || undefined
  return resolvedLinks.find((entry) => entry.contentId === target.contentId)?.href
}

export function mediaAssetHref(
  asset: MediaUseReference | undefined | null,
  resolvedMedia: ResolvedMedia[],
): string | undefined {
  if (!asset) return undefined
  const resolved = resolvedMedia.find((entry) => entry.assetId === asset.asset.assetId)
  if (resolved) return publicMediaHref(resolved.publicUrl)
  reportMissingMedia(asset.asset.assetId, 'inline')
  return undefined
}

export function downloadAssetHref(
  asset: { assetId: string } | undefined | null,
  resolvedMedia: ResolvedMedia[],
): string | undefined {
  if (!asset) return undefined
  const resolved = resolvedMedia.find((entry) => entry.assetId === asset.assetId)
  if (resolved) return publicMediaHref(resolved.downloadUrl)
  reportMissingMedia(asset.assetId, 'download')
  return undefined
}

function reportMissingMedia(assetId: string, use: 'inline' | 'download'): void {
  console.error('[public-media-resolution] Published media mapping is unavailable.', {
    assetId,
    use,
  })
}

export function mediaAlt(asset: MediaUseReference | undefined | null): string {
  if (!asset || asset.decorative) return ''
  return asset.altText?.trim() ?? ''
}

export function isDecorative(asset: MediaUseReference | undefined | null): boolean {
  return Boolean(asset?.decorative)
}
