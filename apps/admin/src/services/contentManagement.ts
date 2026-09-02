import type { ContentKind } from '@airtek/contracts'

/**
 * News owns additional metadata and must use the dedicated News editor/API.
 * Keep this guard in the client as defense in depth even though the API omits
 * News from the generic content collection.
 */
export function isGenericContentKind(kind: ContentKind): boolean {
  return kind !== 'news'
}
