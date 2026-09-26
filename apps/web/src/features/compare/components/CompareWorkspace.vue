<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import type { Product } from '@airtek/contracts'
import { getPublishedProduct } from '@/shared/lib/api'
import { useCompareStore } from '@/features/compare/stores/compare'
import DataNotice from '@/shared/components/common/DataNotice.vue'
import type { ProductFamilyProjection } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

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
const { locale, t } = useI18n({ useScope: 'global' })

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
  return serialized.length <= 160 ? serialized : t('compare.structuredValue')
}

function specificationValue(product: Product, key: string): string {
  const spec = product.specifications.find((candidate) => candidate.key === key)
  if (!spec) return t('common.notPublished')
  const state = ({
    verified: t('compare.verified'), missing: t('common.notPublished'), notApplicable: t('compare.notApplicable'),
    notTested: t('compare.notTested'), confidential: t('compare.confidential'), pendingVerification: t('compare.pendingVerification'),
  })[spec.state]
  const value = spec.value === undefined || spec.value === null ? state : textValue(spec.value)
  return [value, spec.unit, state, spec.operatingCondition].filter(Boolean).join(' · ')
}

function curveSummary(product: Product): string {
  const curves = product.performanceCurves.filter((candidate) => candidate.state === 'verified')
  if (!curves.length) return t('compare.noCurve')
  const conditions = curves.map((curve, index) => {
    const published = [
      curve.speedRpm ? `${curve.speedRpm} rpm` : '', curve.densityKgM3 ? `${curve.densityKgM3} kg/m³` : '',
      curve.voltage || '', curve.testMethod || '',
    ].filter(Boolean).join(' / ') || 'conditions not published'
    return `Curve ${index + 1}: ${curve.points.length} points; ${published}`
  })
  return `${curves.length} verified ${curves.length === 1 ? 'curve' : 'curves'} · ${conditions.join(' | ')}`
}

function row(key: string, label: string, values: string[]): ComparisonRow {
  return { key, label, values, different: new Set(values).size > 1 }
}

const rows = computed<ComparisonRow[]>(() => {
  const selected = displayedProducts.value
  const base = [
    row('stableId', t('compare.stableId'), selected.map((product) => product.stableId)),
    row('model', t('compare.model'), selected.map((product) => product.model || t('common.notPublished'))),
    row('family', t('compare.fanForm'), selected.map((product) => familyLabel(product.family))),
    row('motorTechnology', t('compare.motorTechnology'), selected.map((product) => product.motorTechnology || t('common.notPublished'))),
    row('revision', t('compare.revision'), selected.map((product) => String(product.publishedRevision))),
    row('curve', t('compare.curves'), selected.map(curveSummary)),
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
  loadError.value = rejected ? t('compare.loadError', { count: rejected }, rejected) : ''
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
    shareStatus.value = t('compare.copied')
  } catch {
    shareStatus.value = t('compare.copyBlocked')
  }
}

function csvCell(value: string): string {
  return `"${value.replaceAll('"', '""')}"`
}

function exportCsv(): void {
  const lines = [
    [t('compare.field'), ...displayedProducts.value.map((product) => product.title)].map(csvCell).join(','),
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
  <section class="section shell compare-workspace" :lang="locale">
    <DataNotice
      :title="t('compare.noticeTitle')"
      :text="t('compare.noticeText')"
    />
    <p v-if="loading" class="review-note" role="status">{{ t('compare.loading') }}</p>
    <p v-if="loadError" class="form-error" role="alert">{{ loadError }}</p>

    <div v-if="!loading && !items.length" class="empty-state large">
      <p class="eyebrow">{{ t('compare.noSelection') }}</p>
      <h2>{{ t('compare.build') }}</h2>
      <p>{{ t('compare.addRecords') }}</p>
      <a class="button" href="/en/products">{{ t('compare.browse') }}</a>
    </div>

    <div v-else-if="!loading && !displayedProducts.length" class="empty-state large">
      <p class="eyebrow">{{ t('compare.unavailable') }}</p>
      <h2>{{ t('compare.nonePublished') }}</h2>
      <p>{{ t('compare.noCached') }}</p>
      <button class="button secondary" type="button" @click="compare.clear()">{{ t('compare.clearUnavailable') }}</button>
    </div>

    <div v-else-if="!loading && displayedProducts.length" class="comparison-table-wrap">
      <div class="comparison-toolbar">
        <p><strong>{{ t('compare.publishedRecords', { count: displayedProducts.length }) }}</strong><span v-if="rows.some((entry) => entry.different)">{{ t('compare.differences') }}</span></p>
        <div class="button-row">
          <button class="button secondary" type="button" @click="copyShareUrl">{{ t('compare.copyUrl') }}</button>
          <button class="button secondary" type="button" @click="printComparison">{{ t('compare.print') }}</button>
          <button class="button secondary" type="button" @click="exportCsv">{{ t('compare.exportCsv') }}</button>
        </div>
        <p class="share-status" aria-live="polite">{{ shareStatus }}</p>
      </div>
      <table class="comparison-table">
        <caption>{{ t('compare.caption') }}</caption>
        <thead>
          <tr>
            <th scope="col">{{ t('compare.field') }}</th>
            <th v-for="product in displayedProducts" :key="product.id" scope="col" lang="en">
              <a v-if="product.seo.canonicalPath" :href="product.seo.canonicalPath">{{ product.title }}</a>
              <span v-else>{{ product.title }}</span>
              <small>{{ product.stableId }}</small>
              <button class="table-remove" type="button" :lang="locale" @click="compare.remove(product.id)">{{ t('compare.remove') }}</button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="entry in rows" :key="entry.key" :class="{ 'is-different': entry.different }">
            <th scope="row">{{ entry.label }}<span v-if="entry.different" class="difference-label">{{ t('compare.different') }}</span></th>
            <td v-for="(value, index) in entry.values" :key="displayedProducts[index]?.id" lang="en">{{ value }}</td>
          </tr>
        </tbody>
      </table>
      <div class="button-row comparison-actions">
        <button class="button secondary" type="button" @click="compare.clear()">{{ t('compare.clear') }}</button>
        <a class="button" href="/en/request-a-quote/selection">{{ t('compare.engineeringReview') }}</a>
      </div>
    </div>
  </section>
</template>
