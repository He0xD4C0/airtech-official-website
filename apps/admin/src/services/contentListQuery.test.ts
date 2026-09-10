import { describe, expect, it } from 'vitest'
import {
  buildContentListRouteQuery,
  parseContentListQuery,
  toggleKindSelection,
} from './contentListQuery'

describe('content list URL query', () => {
  it('parses server-side filters from the URL, including the News view', () => {
    expect(parseContentListQuery({ kind: 'news' })).toEqual({
      q: '',
      kinds: ['news'],
      status: undefined,
      sort: 'updatedAt',
      direction: 'desc',
    })
    expect(parseContentListQuery({ kind: 'page,article', sort: 'title' })).toEqual({
      q: '',
      kinds: ['page', 'article'],
      status: undefined,
      sort: 'title',
      direction: 'asc',
    })
  })

  it('ignores unknown status and sort values instead of forwarding them', () => {
    const parsed = parseContentListQuery({ status: 'scheduled', sort: 'drop-table', direction: 'sideways' })
    expect(parsed.status).toBeUndefined()
    expect(parsed.sort).toBe('updatedAt')
    expect(parsed.direction).toBe('desc')
  })

  it('omits default sort and direction so stable URLs stay short', () => {
    const current = parseContentListQuery({})
    expect(buildContentListRouteQuery(current, { kinds: ['news'] })).toEqual({ kind: 'news' })
    expect(buildContentListRouteQuery(current, { sort: 'title' })).toEqual({ sort: 'title' })
    expect(buildContentListRouteQuery(current, { direction: 'asc' })).toEqual({ direction: 'asc' })
  })

  it('keeps the kind filter when other filters change', () => {
    const current = parseContentListQuery({ kind: 'news', status: 'draft' })
    expect(buildContentListRouteQuery(current, { q: 'airtek' })).toEqual({
      q: 'airtek',
      kind: 'news',
      status: 'draft',
    })
  })

  it('toggles kind selection without duplicates', () => {
    expect(toggleKindSelection(['news'], 'news')).toEqual([])
    expect(toggleKindSelection(['news'], 'page')).toEqual(['news', 'page'])
  })
})
