import type { ContentDraftV2, ContentTemplateDefinition } from '@airtek/contracts'

/**
 * Preview-only canonical derivation for the Admin editor.
 *
 * The public SSR route resolver remains the authority for published URLs; this
 * helper exists so editors never type a canonical path by hand. It mirrors the
 * route families the public site currently serves.
 */
export function canonicalPathForDraft(
  draft: Pick<ContentDraftV2, 'kind' | 'locale' | 'slug'>,
  template?: Pick<ContentTemplateDefinition, 'routable'> | null,
): string | null {
  if (template && !template.routable) return null
  const locale = normalizeLocale(draft.locale)
  if (draft.kind === 'home') return `/${locale}`
  const slug = normalizeSlug(draft.slug)
  if (!slug) return null
  if (draft.kind === 'news') return `/${locale}/resources/news/${slug}`
  return `/${locale}/${slug}`
}

export function normalizeSlug(value: string | null | undefined): string | null {
  const slug = (value ?? '').trim().replace(/^\/+|\/+$/gu, '')
  return slug ? slug : null
}

export function normalizeLocale(value: string | null | undefined): string {
  const locale = (value ?? '').trim().toLowerCase()
  return /^[a-z]{2}(?:-[a-z0-9]{2,8})?$/u.test(locale) ? locale : 'en'
}
