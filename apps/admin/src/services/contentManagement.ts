import type { ContentKind } from '@airtek/contracts'

/** News has a specialized editor but shares the unified CMS content service. */
export function isGenericContentKind(kind: ContentKind): boolean {
  return kind !== 'news'
}
