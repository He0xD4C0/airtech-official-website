import type { ContentDraftV2, ContentTemplateDefinition } from '@airtek/contracts'

const LOCALE_TOKEN = '{locale}'
const SLUG_TOKEN = '{slug}'
const MAX_ROUTE_PATTERN_LENGTH = 512
const STATIC_SEGMENT_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u
const SLUG_SEGMENT_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u

/**
 * Preview-only canonical derivation for the Admin editor.
 *
 * The public SSR route resolver remains the authority for published URLs; this
 * helper exists so editors never type a canonical path by hand. The template
 * registry supplies the route pattern so Admin does not mirror route families.
 */
export function canonicalPathForDraft(
  draft: Pick<ContentDraftV2, 'locale' | 'slug'>,
  template?: Pick<ContentTemplateDefinition, 'routable' | 'routePattern'> | null,
): string | null {
  if (!template?.routable) return null
  const route = parseRoutePattern(template.routePattern)
  if (!route) return null
  const locale = normalizeLocale(draft.locale)
  if (!locale) return null
  const slug = normalizeSlug(draft.slug)
  if (route.usesSlug && (!slug || slug.length > 180 || !SLUG_SEGMENT_PATTERN.test(slug))) {
    return null
  }
  const segments = route.segments.map((segment) => {
    if (segment === LOCALE_TOKEN) return locale
    if (segment === SLUG_TOKEN) return slug as string
    return segment
  })
  return `/${segments.join('/')}`
}

export function routePatternUsesSlug(pattern: string | null | undefined): boolean {
  return parseRoutePattern(pattern)?.usesSlug ?? false
}

export function normalizeSlug(value: string | null | undefined): string | null {
  const slug = (value ?? '').trim().replace(/^\/+|\/+$/gu, '')
  return slug ? slug : null
}

export function normalizeLocale(value: string | null | undefined): string | null {
  const locale = (value ?? '').trim().toLowerCase()
  return /^[a-z]{2}(?:-[a-z0-9]{2,8})?$/u.test(locale) ? locale : null
}

function parseRoutePattern(
  pattern: string | null | undefined,
): { segments: string[]; usesSlug: boolean } | null {
  if (
    !pattern
    || pattern.length > MAX_ROUTE_PATTERN_LENGTH
    || !pattern.startsWith('/')
    || pattern.endsWith('/')
    || pattern.includes('//')
  ) return null

  const segments = pattern.slice(1).split('/')
  let localeCount = 0
  let slugCount = 0
  for (const segment of segments) {
    if (segment === LOCALE_TOKEN) localeCount += 1
    else if (segment === SLUG_TOKEN) slugCount += 1
    else if (!STATIC_SEGMENT_PATTERN.test(segment)) return null
  }
  if (localeCount !== 1 || slugCount > 1) return null
  return { segments, usesSlug: slugCount === 1 }
}
