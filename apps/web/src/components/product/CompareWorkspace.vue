<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import type { Product } from '@airtek/contracts'
import { getPublishedProduct } from '@/lib/api'
import { useCompareStore } from '@/stores/compare'
import DataNotice from '@/components/common/DataNotice.vue'
import type { ProductFamilyProjection } from '@/types/content'

interface ComparisonRow {
  key: string
  label: string
  values: string[]
  different: boolean
}

interface ProductLookup {
  slug: string
  family?: Product['family']
}

const validProductFamilies = new Set<Product['family']>(['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors'])

const props = defineProps<{ productFamilies: ProductFamilyProjection[] }>()

const compare = useCompareStore()
const { items } = storeToRefs(compare)
const products = ref<Product[]>([])
const loading = ref(true)
const loadError = ref('')
const shareStatus = ref('')
const hydrating = ref(true)

function familyLabel(code: Product['family']): string {
  return props.productFamilies.find((family) => family.code === code)?.name ?? '—'
}

const displayedProducts = computed(() => items.value.flatMap((item) => {
  const product = products.value.find((candidate) => candidate.id === item.id)
  return product ? [product] : []
}))

function textValue(value: unknown): string {
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return String(value)
  if (value === null || value === undefined) return '—'
  const serialized = JSON.stringify(value)
  return serialized.length <= 160 ? serialized : 'Structured published value'
}

function specificationValue(product: Product, key: string): string {
  const spec = product.specifications.find((candidate) => candidate.key === key)
  if (!spec) return '—'
  const value = spec.value === undefined || spec.value === null ? spec.state : textValue(spec.value)
  return [value, spec.unit, spec.operatingCondition].filter(Boolean).join(' · ')
}

function row(key: string, label: string, values: string[]): ComparisonRow {
  return { key, label, values, different: new Set(values).size > 1 }
}

const rows = computed<ComparisonRow[]>(() => {
  const selected = displayedProducts.value
  const base = [
    row('stableId', 'Stable ID', selected.map((product) => product.stableId)),
    row('model', 'Model', selected.map((product) => product.model || 'Not published')),
    row('family', 'Fan form', selected.map((product) => familyLabel(product.family))),
    row('motorTechnology', 'Motor technology', selected.map((product) => product.motorTechnology || 'Not published')),
    row('revision', 'Published revision', selected.map((product) => String(product.publishedRevision))),
    row('curve', 'Verified PQ curve', selected.map((product) => {
      const curve = product.performanceCurves.find((candidate) => candidate.state === 'verified')
      return curve ? `${curve.points.length} published points` : 'No verified curve'
    })),
  ]
  const specificationLabels = new Map<string, string>()
  for (const product of selected) {
    for (const spec of product.specifications) specificationLabels.set(spec.key, spec.label)
  }
  const specificationRows = [...specificationLabels.entries()]
    .sort((left, right) => left[1].localeCompare(right[1]))
    .map(([key, label]) => row(`spec:${key}`, label, selected.map((product) => specificationValue(product, key))))
  return [...base, ...specificationRows]
})

function sharedProductLookups(): ProductLookup[] {
  const raw = new URL(window.location.href).searchParams.get('products') || ''
  if (raw.length > 800) return []
  const seen = new Set<string>()
  return raw.split(',').flatMap((token): ProductLookup[] => {
    const [familyValue, familySlug, extra] = token.split('~')
    const family = familySlug && !extra && validProductFamilies.has(familyValue as Product['family'])
      ? familyValue as Product['family']
      : undefined
    const slug = family ? familySlug : (familySlug === undefined ? familyValue : undefined)
    if (!slug || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(slug)) return []
    const key = `${family ?? ''}:${slug}`
    if (seen.has(key)) return []
    seen.add(key)
    return [{ slug, family }]
  }).slice(0, 4)
}

function syncUrl(): void {
  const url = new URL(window.location.href)
  const productKeys = items.value.map((item) => item.familyCode ? `${item.familyCode}~${item.slug}` : item.slug).slice(0, 4)
  if (productKeys.length) url.searchParams.set('products', productKeys.join(','))
  else url.searchParams.delete('products')
  window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`)
}

async function fetchProducts(lookups: ProductLookup[]): Promise<Product[]> {
  const settled = await Promise.allSettled(lookups.map(({ slug, family }) => getPublishedProduct(slug, family)))
  const rejected = settled.filter((result) => result.status === 'rejected').length
  loadError.value = rejected ? `${rejected} selected published record${rejected === 1 ? '' : 's'} could not be loaded.` : ''
  return settled.flatMap((result) => result.status === 'fulfilled' ? [result.value] : [])
}

async function loadSelected(): Promise<void> {
  loading.value = true
  products.value = await fetchProducts(items.value.map((item) => ({ slug: item.slug, family: item.familyCode })))
  loading.value = false
}

async function initialize(): Promise<void> {
  compare.hydrate()
  const sharedLookups = sharedProductLookups()
  if (sharedLookups.length) {
    compare.clear()
    const sharedProducts = await fetchProducts(sharedLookups)
    for (const product of sharedProducts) {
      compare.add({
        id: product.id,
        slug: product.slug,
        label: product.title,
        family: familyLabel(product.family),
        familyCode: product.family,
        dataState: 'published',
        publishedRevision: product.publishedRevision ?? undefined,
      })
    }
    products.value = sharedProducts
  } else {
    products.value = await fetchProducts(items.value.map((item) => ({ slug: item.slug, family: item.familyCode })))
  }
  hydrating.value = false
  loading.value = false
  syncUrl()
}

async function copyShareUrl(): Promise<void> {
  syncUrl()
  try {
    await window.navigator.clipboard.writeText(window.location.href)
    shareStatus.value = 'Share URL copied.'
  } catch {
    shareStatus.value = 'Copy was blocked; use the current browser URL.'
  }
}

function csvCell(value: string): string {
  return `"${value.replaceAll('"', '""')}"`
}

function exportCsv(): void {
  const lines = [
    ['Field', ...displayedProducts.value.map((product) => product.title)].map(csvCell).join(','),
    ...rows.value.map((entry) => [entry.label, ...entry.values].map(csvCell).join(',')),
  ]
  const url = URL.createObjectURL(new Blob([`\uFEFF${lines.join('\n')}\n`], { type: 'text/csv;charset=utf-8' }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = 'airtekpower-product-comparison.csv'
  anchor.click()
  URL.revokeObjectURL(url)
}

function printComparison(): void {
  window.print()
}

watch(
  () => items.value.map((item) => `${item.id}:${item.familyCode ?? ''}:${item.slug}:${item.publishedRevision ?? ''}`).join('|'),
  () => {
    if (hydrating.value) return
    syncUrl()
    void loadSelected()
  },
  { flush: 'sync' },
)

onMounted(() => { void initialize() })
</script>

<template>
  <section class="section shell compare-workspace">
    <DataNotice
      title="Published-record comparison"
      text="Every column is reloaded from the current Public Product Master projection. Missing or non-verified fields remain explicit; no compatibility score is inferred."
    />
    <p v-if="loading" class="review-note" role="status">Loading selected published records…</p>
    <p v-if="loadError" class="form-error" role="alert">{{ loadError }}</p>

    <div v-if="!loading && !items.length" class="empty-state large">
      <p class="eyebrow">No records selected</p>
      <h2>Build a comparison from a published product page</h2>
      <p>Add up to four records. The resulting URL can be shared without exposing draft or private data.</p>
      <a class="button" href="/en/products">Browse products</a>
    </div>

    <div v-else-if="!loading && !displayedProducts.length" class="empty-state large">
      <p class="eyebrow">Records unavailable</p>
      <h2>No selected record is present in the current published projection.</h2>
      <p>The workspace will not reuse cached specifications or fabricate missing values.</p>
      <button class="button secondary" type="button" @click="compare.clear()">Clear unavailable records</button>
    </div>

    <div v-else-if="!loading && displayedProducts.length" class="comparison-table-wrap">
      <div class="comparison-toolbar">
        <p><strong>{{ displayedProducts.length }}/4 published records</strong><span v-if="rows.some((entry) => entry.different)">Differences are highlighted.</span></p>
        <div class="button-row">
          <button class="button secondary" type="button" @click="copyShareUrl">Copy share URL</button>
          <button class="button secondary" type="button" @click="printComparison">Print</button>
          <button class="button secondary" type="button" @click="exportCsv">Export CSV</button>
        </div>
        <p class="share-status" aria-live="polite">{{ shareStatus }}</p>
      </div>
      <table class="comparison-table">
        <caption>Current published product fields; highlighted rows differ across selected records.</caption>
        <thead>
          <tr>
            <th scope="col">Field</th>
            <th v-for="product in displayedProducts" :key="product.id" scope="col">
              <a :href="`/en/products/${product.family === 'crossFlow' ? 'cross-flow' : product.family === 'inlineDuct' ? 'inline-duct' : product.family}/${product.slug}`">{{ product.title }}</a>
              <small>{{ product.stableId }}</small>
              <button class="table-remove" type="button" @click="compare.remove(product.id)">Remove</button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="entry in rows" :key="entry.key" :class="{ 'is-different': entry.different }">
            <th scope="row">{{ entry.label }}<span v-if="entry.different" class="difference-label">Different</span></th>
            <td v-for="(value, index) in entry.values" :key="displayedProducts[index]?.id">{{ value }}</td>
          </tr>
        </tbody>
      </table>
      <div class="button-row comparison-actions">
        <button class="button secondary" type="button" @click="compare.clear()">Clear comparison</button>
        <a class="button" href="/en/request-a-quote/selection">Request engineering review</a>
      </div>
    </div>
  </section>
</template>
