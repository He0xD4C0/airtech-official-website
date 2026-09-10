import type { ContentSortField, ContentStatusFilter, SortDirection } from './contentApi'

export interface ContentListFilters {
  q: string
  kinds: string[]
  status: ContentStatusFilter | undefined
  sort: ContentSortField
  direction: SortDirection
}

const STATUS_VALUES: ContentStatusFilter[] = ['draft', 'published', 'archived']
const SORT_VALUES: ContentSortField[] = ['updatedAt', 'title', 'kind']

export function defaultDirection(sort: ContentSortField): SortDirection {
  return sort === 'updatedAt' ? 'desc' : 'asc'
}

export function parseContentListQuery(query: Record<string, unknown>): ContentListFilters {
  const statusValue = stringValue(query.status)
  const sortValue = stringValue(query.sort)
  const sort = SORT_VALUES.includes(sortValue as ContentSortField)
    ? sortValue as ContentSortField
    : 'updatedAt'
  const directionValue = stringValue(query.direction)
  return {
    q: stringValue(query.q).trim(),
    kinds: parseKinds(query.kind),
    status: STATUS_VALUES.includes(statusValue as ContentStatusFilter)
      ? statusValue as ContentStatusFilter
      : undefined,
    sort,
    direction: directionValue === 'asc' || directionValue === 'desc'
      ? directionValue
      : defaultDirection(sort),
  }
}

export function buildContentListRouteQuery(
  current: ContentListFilters,
  patch: Partial<ContentListFilters>,
): Record<string, string> {
  const merged: ContentListFilters = { ...current, ...patch }
  if (patch.sort !== undefined && patch.direction === undefined) {
    merged.direction = defaultDirection(patch.sort)
  }
  const next: Record<string, string> = {}
  if (merged.q.trim()) next.q = merged.q.trim()
  if (merged.kinds.length) next.kind = merged.kinds.join(',')
  if (merged.status) next.status = merged.status
  if (merged.sort !== 'updatedAt') next.sort = merged.sort
  if (merged.direction !== defaultDirection(merged.sort)) next.direction = merged.direction
  return next
}

export function toggleKindSelection(kinds: string[], kind: string): string[] {
  return kinds.includes(kind) ? kinds.filter((entry) => entry !== kind) : [...kinds, kind]
}

function parseKinds(value: unknown): string[] {
  const raw = Array.isArray(value) ? value.join(',') : value
  if (typeof raw !== 'string') return []
  return [...new Set(raw.split(',').map((entry) => entry.trim()).filter(Boolean))]
}

function stringValue(value: unknown): string {
  return typeof value === 'string' ? value : ''
}
