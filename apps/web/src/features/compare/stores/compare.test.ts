import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useCompareStore } from '@/features/compare/stores/compare'

describe('compare store', () => {
  beforeEach(() => {
    window.sessionStorage.clear()
    setActivePinia(createPinia())
  })

  it('deduplicates records and enforces the four-record limit', () => {
    const store = useCompareStore()
    for (let index = 0; index < 6; index += 1) {
      store.add({ id: String(index), slug: `record-${index}`, label: `Record ${index}`, family: 'Preview', dataState: 'pendingVerification' })
    }
    store.add({ id: '0', slug: 'duplicate', label: 'Duplicate', family: 'Preview', dataState: 'pendingVerification' })
    expect(store.items).toHaveLength(4)
    expect(store.items[0].label).toBe('Record 0')
  })

  it('persists only the compare workspace in session storage', () => {
    const store = useCompareStore()
    store.add({ id: 'one', slug: 'preview', label: 'Preview', family: 'Axial fans', dataState: 'pendingVerification' })
    const serialized = window.sessionStorage.getItem('airtek.public.compare.v1')
    expect(serialized).toContain('Axial fans')
  })

  it('drops malformed session data instead of rendering arbitrary records', () => {
    window.sessionStorage.setItem('airtek.public.compare.v1', JSON.stringify([
      { id: 'unsafe', slug: '../admin', label: '<img>', family: 'Unknown', dataState: 'published' },
    ]))
    const store = useCompareStore()
    store.hydrate()
    expect(store.items).toEqual([])
  })
})
