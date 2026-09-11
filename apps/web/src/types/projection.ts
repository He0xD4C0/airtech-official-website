import type {
  LinkTargetReference,
  MediaUseReference,
  PublicContentProjection as GeneratedPublicContentProjection,
  ResolvedLinkTarget as GeneratedResolvedLinkTarget,
  ResolvedRelationCard as GeneratedResolvedRelationCard,
} from '@airtek/contracts'

export type ResolvedRelationCard = GeneratedResolvedRelationCard
export type ResolvedLinkTarget = GeneratedResolvedLinkTarget

/**
 * Native V2 public projection served by the platform API. The server has
 * already resolved relations and content links so the browser never needs
 * database access or UUID guessing.
 */
export type PublicContentProjection = GeneratedPublicContentProjection

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
): string | undefined {
  if (!asset) return undefined
  return `/media/${asset.asset.assetId}`
}

export function downloadAssetHref(
  asset: { assetId: string; versionId: string } | undefined | null,
): string | undefined {
  if (!asset) return undefined
  return `/media/${asset.assetId}/download`
}

export function mediaAlt(asset: MediaUseReference | undefined | null): string {
  if (!asset || asset.decorative) return ''
  return asset.altText?.trim() ?? ''
}

export function isDecorative(asset: MediaUseReference | undefined | null): boolean {
  return Boolean(asset?.decorative)
}
