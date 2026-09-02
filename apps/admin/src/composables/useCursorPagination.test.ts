import { describe, expect, it, vi } from 'vitest'
import { useCursorPagination } from './useCursorPagination'
import { apiProblemStatus, buildCursorPagePath, collectCursorPages, findInCursorPages } from '@/services/cursorPagination'

describe('cursor pagination', () => {
  it('encodes opaque cursors and validates the server limit contract', () => {
    expect(buildCursorPagePath('/content', { cursor: 'opaque+/=', limit: 25 }))
      .toBe('/content?cursor=opaque%2B%2F%3D&limit=25')
    expect(buildCursorPagePath('/content')).toBe('/content')
    expect(() => buildCursorPagePath('/content', { limit: 0 })).toThrow(RangeError)
    expect(() => buildCursorPagePath('/content', { limit: 101 })).toThrow(RangeError)
  })

  it('uses server cursors for next and remembered cursors for previous', async () => {
    const fetchPage = vi.fn(async ({ cursor }: { cursor?: string | null }) => cursor === 'after-2'
      ? { items: [3], nextCursor: null }
      : { items: [1, 2], nextCursor: 'after-2' })
    const pager = useCursorPagination(fetchPage, { pageSize: 2 })

    expect(await pager.first()).toBe(true)
    expect(pager.items.value).toEqual([1, 2])
    expect(pager.canNext.value).toBe(true)

    expect(await pager.next()).toBe(true)
    expect(pager.items.value).toEqual([3])
    expect(pager.pageNumber.value).toBe(2)
    expect(pager.canPrevious.value).toBe(true)

    expect(await pager.previous()).toBe(true)
    expect(pager.items.value).toEqual([1, 2])
    expect(fetchPage).toHaveBeenNthCalledWith(1, { cursor: null, limit: 2 })
    expect(fetchPage).toHaveBeenNthCalledWith(2, { cursor: 'after-2', limit: 2 })
    expect(fetchPage).toHaveBeenNthCalledWith(3, { cursor: null, limit: 2 })
  })

  it('keeps the current page when the next cursor fails', async () => {
    const fetchPage = vi.fn(async ({ cursor }: { cursor?: string | null }) => {
      if (cursor) throw { title: '游标已失效', status: 400 }
      return { items: ['first'], nextCursor: 'expired' }
    })
    const onError = vi.fn()
    const pager = useCursorPagination(fetchPage, { onError })

    await pager.first()
    expect(await pager.next()).toBe(false)
    expect(pager.items.value).toEqual(['first'])
    expect(pager.pageNumber.value).toBe(1)
    expect(pager.error.value).toBe('游标已失效')
    expect(pager.errorStatus.value).toBe(400)
    expect(onError).toHaveBeenCalledWith('游标已失效')
  })

  it('follows cursors when an editor opens an item outside the first page', async () => {
    const fetchPage = vi.fn(async ({ cursor }: { cursor?: string | null }) => cursor
      ? { items: [{ id: 'target' }], nextCursor: null }
      : { items: [{ id: 'first' }], nextCursor: 'after-first' })

    await expect(findInCursorPages(fetchPage, (item) => item.id === 'target')).resolves.toEqual({ id: 'target' })
    expect(fetchPage).toHaveBeenNthCalledWith(2, { cursor: 'after-first', limit: 100 })
  })

  it('collects every page and rejects a repeated cursor', async () => {
    const fetchPage = vi.fn(async ({ cursor }: { cursor?: string | null }) => cursor === 'after-first'
      ? { items: [2, 3], nextCursor: null }
      : { items: [1], nextCursor: 'after-first' })
    await expect(collectCursorPages(fetchPage)).resolves.toEqual([1, 2, 3])

    const repeated = vi.fn(async () => ({ items: [] as number[], nextCursor: 'same-cursor' }))
    await expect(collectCursorPages(repeated)).rejects.toThrow('重复游标')
  })

  it('reads status from generated contract client errors', () => {
    expect(apiProblemStatus({ problem: { status: 403 } })).toBe(403)
    expect(apiProblemStatus({ status: 409 })).toBe(409)
  })
})
