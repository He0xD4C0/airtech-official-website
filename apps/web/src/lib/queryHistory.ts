import { onMounted, onUnmounted } from 'vue'

// Preserve the router's state; keep the cursor chain private to this history entry.
export function useQueryHistory(restore: (params: URLSearchParams, cursors: Array<string | null>) => void) {
  function onPop() {
    const params = new URLSearchParams(window.location.search)
    const saved: unknown = window.history.state?.publicQueryCursors
    const cursors = Array.isArray(saved) && saved.every((entry) => entry === null || typeof entry === 'string')
      ? saved as Array<string | null> : [params.get('cursor')]
    restore(params, cursors)
  }
  onMounted(() => window.addEventListener('popstate', onPop))
  onUnmounted(() => window.removeEventListener('popstate', onPop))
  return (values: Record<string, string | undefined>, cursors: Array<string | null>) => {
    const url = new URL(window.location.href)
    for (const [key, value] of Object.entries(values)) {
      if (value) url.searchParams.set(key, value)
      else url.searchParams.delete(key)
    }
    if (url.href === window.location.href) return
    // Vue refs expose proxy arrays; History's structured clone rejects proxies.
    window.history.pushState({ ...window.history.state, publicQueryCursors: Array.from(cursors) }, '', url)
  }
}
