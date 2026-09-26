import { defineStore } from 'pinia'
import type { ProductFamily } from '@airtek/contracts'
import { trackAnalyticsEvent } from '@/features/analytics'

export interface CompareItem {
  id: string
  slug: string
  label: string
  family: string
  familyCode?: ProductFamily
  dataState: 'published' | 'pendingVerification'
  publishedRevision?: number
}

const storageKey = 'airtek.public.compare.v1'

function validItem(value: unknown): value is CompareItem {
  if (!value || typeof value !== 'object') return false
  const item = value as Partial<CompareItem>
  return typeof item.id === 'string'
    && item.id.length <= 80
    && typeof item.slug === 'string'
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(item.slug)
    && typeof item.label === 'string'
    && item.label.length <= 180
    && typeof item.family === 'string'
    && item.family.length <= 80
    && (item.familyCode === undefined || ['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'].includes(item.familyCode))
    && (item.dataState === 'published' || item.dataState === 'pendingVerification')
    && (item.publishedRevision === undefined || (Number.isInteger(item.publishedRevision) && item.publishedRevision > 0))
}

export const useCompareStore = defineStore('compare', {
  state: () => ({
    items: [] as CompareItem[],
    hydrated: false,
  }),
  actions: {
    hydrate() {
      if (this.hydrated || typeof window === 'undefined') return
      this.hydrated = true
      try {
        const value = window.sessionStorage.getItem(storageKey)
        const parsed: unknown = value ? JSON.parse(value) : []
        this.items = Array.isArray(parsed) ? parsed.filter(validItem).slice(0, 4) : []
      } catch {
        this.items = []
      }
    },
    add(item: CompareItem) {
      if (this.items.some((entry) => entry.id === item.id) || this.items.length >= 4) return
      this.items.push(item)
      this.persist()
      void trackAnalyticsEvent('compareChanged', { action: 'add', itemCount: this.items.length, productId: item.id })
    },
    remove(id: string) {
      this.items = this.items.filter((item) => item.id !== id)
      this.persist()
      void trackAnalyticsEvent('compareChanged', { action: 'remove', itemCount: this.items.length, productId: id })
    },
    clear() {
      this.items = []
      this.persist()
      void trackAnalyticsEvent('compareChanged', { action: 'clear', itemCount: 0 })
    },
    persist() {
      if (typeof window !== 'undefined') {
        window.sessionStorage.setItem(storageKey, JSON.stringify(this.items))
      }
    },
  },
})
