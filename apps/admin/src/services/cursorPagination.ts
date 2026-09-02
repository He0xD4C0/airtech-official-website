export interface CursorPage<T> {
  items: T[]
  nextCursor: string | null
}

export interface CursorPageRequest {
  cursor?: string | null
  limit?: number
}

export const DEFAULT_ADMIN_PAGE_SIZE = 25

export function buildCursorPagePath(path: string, request: CursorPageRequest = {}): string {
  const search = new URLSearchParams()
  if (request.cursor) search.set('cursor', request.cursor)
  if (request.limit !== undefined) {
    if (!Number.isInteger(request.limit) || request.limit < 1 || request.limit > 100) {
      throw new RangeError('分页大小必须是 1 到 100 之间的整数。')
    }
    search.set('limit', String(request.limit))
  }
  const query = search.toString()
  return query ? `${path}?${query}` : path
}

export function apiErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message
  if (error && typeof error === 'object') {
    const problem = error as { detail?: unknown; title?: unknown }
    if (typeof problem.detail === 'string' && problem.detail) return problem.detail
    if (typeof problem.title === 'string' && problem.title) return problem.title
  }
  return fallback
}

export function apiProblemStatus(error: unknown): number | undefined {
  if (!error || typeof error !== 'object') return undefined
  const direct = 'status' in error ? (error as { status?: unknown }).status : undefined
  const nested = 'problem' in error && error.problem && typeof error.problem === 'object' && 'status' in error.problem
    ? (error.problem as { status?: unknown }).status
    : undefined
  const status = Number(direct ?? nested)
  return Number.isInteger(status) ? status : undefined
}

export async function findInCursorPages<T>(
  fetchPage: (request: CursorPageRequest) => Promise<CursorPage<T>>,
  predicate: (item: T) => boolean,
  limit = 100,
): Promise<T | null> {
  const seen = new Set<string>()
  let cursor: string | null = null
  while (true) {
    const page = await fetchPage({ cursor, limit })
    const match = page.items.find(predicate)
    if (match) return match
    if (!page.nextCursor) return null
    if (seen.has(page.nextCursor)) throw new Error('列表 API 返回了重复游标。')
    seen.add(page.nextCursor)
    cursor = page.nextCursor
  }
}

export async function collectCursorPages<T>(
  fetchPage: (request: CursorPageRequest) => Promise<CursorPage<T>>,
  limit = 100,
): Promise<T[]> {
  const items: T[] = []
  const seen = new Set<string>()
  let cursor: string | null = null
  while (true) {
    const page = await fetchPage({ cursor, limit })
    items.push(...page.items)
    if (!page.nextCursor) return items
    if (seen.has(page.nextCursor)) throw new Error('列表 API 返回了重复游标。')
    seen.add(page.nextCursor)
    cursor = page.nextCursor
  }
}
