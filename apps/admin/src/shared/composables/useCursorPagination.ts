import { computed, shallowRef } from 'vue'
import { apiErrorMessage, apiProblemStatus, DEFAULT_ADMIN_PAGE_SIZE, type CursorPage, type CursorPageRequest } from '@/shared/services/cursorPagination'

interface CursorPaginationOptions {
  pageSize?: number
  errorMessage?: string
  onError?: (message: string) => void
}

export function useCursorPagination<T>(
  fetchPage: (request: CursorPageRequest) => Promise<CursorPage<T>>,
  options: CursorPaginationOptions = {},
) {
  const pageSize = options.pageSize ?? DEFAULT_ADMIN_PAGE_SIZE
  if (!Number.isInteger(pageSize) || pageSize < 1 || pageSize > 100) {
    throw new RangeError('分页大小必须是 1 到 100 之间的整数。')
  }

  const items = shallowRef<T[]>([])
  const nextCursor = shallowRef<string | null>(null)
  const cursorHistory = shallowRef<Array<string | null>>([null])
  const pageIndex = shallowRef(0)
  const loading = shallowRef(false)
  const error = shallowRef<string | null>(null)
  const errorStatus = shallowRef<number | null>(null)
  let requestSequence = 0

  async function load(cursor: string | null, targetIndex: number): Promise<boolean> {
    const requestId = ++requestSequence
    loading.value = true
    error.value = null
    errorStatus.value = null
    try {
      const page = await fetchPage({ cursor, limit: pageSize })
      if (requestId !== requestSequence) return false

      items.value = page.items
      nextCursor.value = page.nextCursor
      pageIndex.value = targetIndex

      const nextHistory = cursorHistory.value.slice(0, targetIndex + 1)
      nextHistory[targetIndex] = cursor
      if (page.nextCursor) nextHistory[targetIndex + 1] = page.nextCursor
      cursorHistory.value = nextHistory
      return true
    } catch (cause) {
      if (requestId !== requestSequence) return false
      const message = apiErrorMessage(cause, options.errorMessage ?? '列表读取失败。')
      error.value = message
      errorStatus.value = apiProblemStatus(cause) ?? null
      options.onError?.(message)
      return false
    } finally {
      if (requestId === requestSequence) loading.value = false
    }
  }

  async function first(): Promise<boolean> {
    return load(null, 0)
  }

  async function next(): Promise<boolean> {
    if (loading.value || !nextCursor.value) return false
    return load(nextCursor.value, pageIndex.value + 1)
  }

  async function previous(): Promise<boolean> {
    if (loading.value || pageIndex.value === 0) return false
    const targetIndex = pageIndex.value - 1
    return load(cursorHistory.value[targetIndex] ?? null, targetIndex)
  }

  async function refresh(): Promise<boolean> {
    return load(cursorHistory.value[pageIndex.value] ?? null, pageIndex.value)
  }

  return {
    items,
    nextCursor,
    pageNumber: computed(() => pageIndex.value + 1),
    canPrevious: computed(() => !loading.value && pageIndex.value > 0),
    canNext: computed(() => !loading.value && Boolean(nextCursor.value)),
    loading,
    error,
    errorStatus,
    first,
    next,
    previous,
    refresh,
  }
}
