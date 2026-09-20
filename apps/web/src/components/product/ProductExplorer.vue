<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Product, ProductFacetCount, ProductFamily as ProductFamilyValue } from '@airtek/contracts'
import SectionHeading from '@/components/common/SectionHeading.vue'
import { useQueryHistory } from '@/lib/queryHistory'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { getPublishedProducts } from '@/lib/api'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/lib/productPagination'
import { useCompareStore } from '@/stores/compare'
import type { ProductFamilyProjection, PublicPageModel } from '@/types/content'

type CatalogState = NonNullable<PublicPageModel['catalogState']>

const props = withDefaults(defineProps<{
  category?: string
  families: ProductFamilyProjection[]
  products: Product[]
  nextCursor?: string | null
  total?: number
  familyCounts?: ProductFacetCount[]
  motorTechnologyCounts?: ProductFacetCount[]
  initialState?: CatalogState
}>(), {
  nextCursor: null,
  total: 0,
  familyCounts: () => [],
  motorTechnologyCounts: () => [],
  initialState: () => ({ q: '', view: 'cards' }),
})

const familiesByCode = computed(() => new Map(props.families.map((family) => [family.code, family])))
const lockedFamily = computed(() => props.families.find((family) => family.slug === props.category)?.code)
const query = ref(props.initialState.q)
const selectedFamily = ref<ProductFamilyValue | 'all'>(lockedFamily.value ?? props.initialState.family ?? 'all')
const selectedMotor = ref(props.initialState.motorTechnology ?? 'all')
const view = ref<'cards' | 'table'>(props.initialState.view)
const pageProducts = ref<Product[]>([...props.products])
const pageNextCursor = ref<string | null>(props.nextCursor ?? null)
const resultTotal = ref(props.total)
const familyCounts = ref([...props.familyCounts])
const motorTechnologyCounts = ref([...props.motorTechnologyCounts])
const cursorHistory = ref<Array<string | null>>([props.initialState.cursor ?? null])
const pageIndex = ref(0)
const loading = ref(false)
const pageError = ref('')
let requestGeneration = 0
const compare = useCompareStore()

const publishedProducts = computed(() => pageProducts.value.filter((product) => (
  product.status === 'published'
  && typeof product.publishedRevision === 'number'
  && product.locale === 'en'
  && familiesByCode.value.has(product.family)
)))
const familyCountMap = computed(() => new Map(familyCounts.value.map((facet) => [facet.value, facet.count])))
const motorTechnologies = computed(() => motorTechnologyCounts.value.map((facet) => facet.value))

watch(
  () => [props.products, props.nextCursor, props.total, props.familyCounts, props.motorTechnologyCounts, props.initialState, props.category] as const,
  () => {
    requestGeneration += 1
    pageProducts.value = [...props.products]
    pageNextCursor.value = props.nextCursor ?? null
    resultTotal.value = props.total
    familyCounts.value = [...props.familyCounts]
    motorTechnologyCounts.value = [...props.motorTechnologyCounts]
    cursorHistory.value = [props.initialState.cursor ?? null]
    pageIndex.value = 0
    query.value = props.initialState.q
    selectedFamily.value = lockedFamily.value ?? props.initialState.family ?? 'all'
    selectedMotor.value = props.initialState.motorTechnology ?? 'all'
    view.value = props.initialState.view
    loading.value = false
    pageError.value = ''
  },
)

function familyName(product: Product): string {
  return familiesByCode.value.get(product.family)?.name ?? product.family
}

function productHref(product: Product): string {
  const family = familiesByCode.value.get(product.family)
  return product.seo.canonicalPath || `/en/products/${family?.slug ?? product.family}/${product.slug}`
}

function verifiedSpec(product: Product, key: string): string | undefined {
  const spec = product.specifications.find((entry) => entry.key === key && entry.state === 'verified')
  if (!spec || !['string', 'number', 'boolean'].includes(typeof spec.value)) return undefined
  return [String(spec.value), spec.unit].filter(Boolean).join(' ')
}

function isCompared(product: Product): boolean {
  return compare.items.some((item) => item.id === product.id)
}

function addToCompare(product: Product): void {
  compare.hydrate()
  compare.add({
    id: product.id, slug: product.slug, label: product.title, family: familyName(product),
    familyCode: product.family, dataState: 'published', publishedRevision: product.publishedRevision ?? undefined,
  })
}

const writeQuery = useQueryHistory((params, cursors) => {
  query.value = params.get('q') ?? ''
  selectedFamily.value = lockedFamily.value ?? props.families.find((family) => family.code === params.get('family'))?.code ?? 'all'
  selectedMotor.value = params.get('motorTechnology') ?? 'all'
  view.value = params.get('view') === 'table' ? 'table' : 'cards'
  cursorHistory.value = cursors
  pageIndex.value = Math.max(0, cursors.indexOf(params.get('cursor')))
  void loadPage(params.get('cursor'))
})

function syncUrl(): void {
  if (typeof window === 'undefined') return
  writeQuery({
    q: query.value.trim(),
    family: selectedFamily.value === 'all' ? undefined : selectedFamily.value,
    motorTechnology: selectedMotor.value === 'all' ? undefined : selectedMotor.value,
    view: view.value,
    cursor: cursorHistory.value[pageIndex.value] ?? undefined,
  }, cursorHistory.value)
}

function trackFilter(filterName: 'catalogSearch' | 'family' | 'motorTechnology' | 'catalogFilters'): void {
  void trackAnalyticsEvent('filterApplied', { filterName, resultCount: resultTotal.value })
}

function productListQuery(cursor: string | null) {
  return {
    limit: PUBLIC_PRODUCT_PAGE_SIZE,
    ...(query.value.trim() ? { q: query.value.trim() } : {}),
    ...(selectedFamily.value !== 'all' ? { family: selectedFamily.value } : {}),
    ...(selectedMotor.value !== 'all' ? { motorTechnology: selectedMotor.value } : {}),
    ...(cursor ? { cursor } : {}),
  }
}

async function loadPage(cursor: string | null): Promise<boolean> {
  const generation = ++requestGeneration
  loading.value = true
  pageError.value = ''
  try {
    const page = await getPublishedProducts(productListQuery(cursor))
    if (generation !== requestGeneration) return false
    pageProducts.value = page.items
    pageNextCursor.value = page.nextCursor
    resultTotal.value = page.total
    familyCounts.value = page.familyCounts
    motorTechnologyCounts.value = page.motorTechnologyCounts
    return true
  } catch (cause) {
    if (generation === requestGeneration) {
      pageError.value = cause instanceof Error ? cause.message : 'The catalog page could not be loaded.'
    }
    return false
  } finally {
    if (generation === requestGeneration) loading.value = false
  }
}

async function resetPagination(filterName?: Parameters<typeof trackFilter>[0]): Promise<void> {
  cursorHistory.value = [null]
  pageIndex.value = 0
  pageNextCursor.value = null
  syncUrl()
  await loadPage(null)
  if (filterName) trackFilter(filterName)
}

async function clearFilters(): Promise<void> {
  query.value = ''
  selectedFamily.value = lockedFamily.value ?? 'all'
  selectedMotor.value = 'all'
  await resetPagination('catalogFilters')
}

function setView(value: 'cards' | 'table'): void {
  view.value = value
  syncUrl()
}

function trackPageChange(): void {
  void trackAnalyticsEvent('filterApplied', { filterName: 'catalogPagination', resultCount: pageProducts.value.length })
}

async function nextPage(): Promise<void> {
  const cursor = pageNextCursor.value
  if (!cursor || loading.value) return
  if (await loadPage(cursor)) {
    cursorHistory.value = [...cursorHistory.value.slice(0, pageIndex.value + 1), cursor]
    pageIndex.value += 1
    syncUrl()
    trackPageChange()
  }
}

async function previousPage(): Promise<void> {
  if (pageIndex.value === 0 || loading.value) return
  const targetIndex = pageIndex.value - 1
  const cursor = cursorHistory.value[targetIndex] ?? null
  if (await loadPage(cursor)) {
    pageIndex.value = targetIndex
    syncUrl()
    trackPageChange()
  }
}
</script>

<template>
  <section class="section shell">
    <SectionHeading eyebrow="Published catalog" title="Products" />
    <div class="filter-panel product-filter-panel" aria-label="Product filters" :aria-busy="loading">
      <label><span>Search the published catalog</span><input v-model="query" type="search" placeholder="Title, model, Stable ID, subtype or specification" @change="resetPagination('catalogSearch')"></label>
      <label><span>Fan form</span><select v-model="selectedFamily" :disabled="Boolean(lockedFamily)" @change="resetPagination('family')">
        <option v-if="!lockedFamily" value="all">All product families</option>
        <option v-for="family in families" :key="family.code" :value="family.code">{{ family.name }} ({{ familyCountMap.get(family.code) ?? 0 }})</option>
      </select></label>
      <label><span>Motor technology</span><select v-model="selectedMotor" @change="resetPagination('motorTechnology')">
        <option value="all">All published technologies</option>
        <option v-for="technology in motorTechnologies" :key="technology" :value="technology">{{ technology }}</option>
      </select></label>
      <p class="result-count" role="status">{{ resultTotal }} matching {{ resultTotal === 1 ? 'record' : 'records' }}</p>
      <p class="pagination-scope">Showing {{ publishedProducts.length }} records on page {{ pageIndex + 1 }}.</p>
    </div>

    <div v-if="pageError" class="data-notice catalog-page-error" role="alert">
      <div><strong>Catalog page unavailable</strong><p>{{ pageError }}</p></div>
      <button class="button secondary" type="button" :disabled="loading" @click="resetPagination()">Reload first page</button>
    </div>

    <div v-if="publishedProducts.length" class="catalog-toolbar" aria-label="Catalog view">
      <span>View</span>
      <button type="button" :aria-pressed="view === 'cards'" @click="setView('cards')">Cards</button>
      <button type="button" :aria-pressed="view === 'table'" @click="setView('table')">Table</button>
    </div>

    <div v-if="publishedProducts.length && view === 'cards'" class="card-grid product-grid">
      <article v-for="product in publishedProducts" :key="product.id" class="card product-card">
        <div class="product-card-body">
          <p class="eyebrow">{{ familyName(product) }}</p>
          <h2><a :href="productHref(product)">{{ product.model || product.title }}</a></h2>
          <dl class="product-card-meta">
            <div><dt>Model</dt><dd>{{ product.model || 'Not published' }}</dd></div>
            <div><dt>Diameter</dt><dd>{{ verifiedSpec(product, 'diameter') || 'Not published' }}</dd></div>
            <div><dt>Rated voltage</dt><dd>{{ verifiedSpec(product, 'voltage') || 'Not published' }}</dd></div>
            <div><dt>Protection</dt><dd>{{ verifiedSpec(product, 'protection') || 'Not published' }}</dd></div>
          </dl>
          <p>{{ product.summary || 'Product summary not published.' }}</p>
          <ul class="tag-list" aria-label="Published facets">
            <li v-if="product.subtype">{{ product.subtype }}</li>
            <li v-if="product.motorTechnology">{{ product.motorTechnology }}</li>
            <li v-else>Motor technology not published</li>
          </ul>
          <div class="product-card-actions">
            <a class="text-link" :href="productHref(product)">View product <span aria-hidden="true">→</span></a>
            <button class="button secondary" type="button" :disabled="isCompared(product)" @click="addToCompare(product)">{{ isCompared(product) ? 'Added' : 'Compare' }}</button>
          </div>
        </div>
      </article>
    </div>

    <div v-else-if="publishedProducts.length" class="table-scroll published-products-table">
      <table>
        <caption>{{ resultTotal }} matching published product records</caption>
        <thead><tr><th scope="col">Product</th><th scope="col">Family</th><th scope="col">Diameter</th><th scope="col">Voltage</th><th scope="col">Protection</th><th scope="col">Motor technology</th></tr></thead>
        <tbody><tr v-for="product in publishedProducts" :key="product.id">
          <th scope="row"><a :href="productHref(product)">{{ product.model || product.title }}</a><small>{{ product.stableId }}</small></th>
          <td>{{ familyName(product) }}</td><td>{{ verifiedSpec(product, 'diameter') || 'Not published' }}</td><td>{{ verifiedSpec(product, 'voltage') || 'Not published' }}</td><td>{{ verifiedSpec(product, 'protection') || 'Not published' }}</td><td>{{ product.motorTechnology || 'Not published' }}</td>
        </tr></tbody>
      </table>
    </div>

    <div v-else class="empty-state large">
      <p class="eyebrow">No matching published records</p><h2>No validated products are available for these filters.</h2>
      <p>No model, performance value or compatibility claim has been inferred.</p>
      <button v-if="query || selectedMotor !== 'all' || (!lockedFamily && selectedFamily !== 'all')" class="button secondary" type="button" @click="clearFilters">Clear filters</button>
    </div>

    <nav v-if="pageIndex > 0 || pageNextCursor" class="catalog-pagination" aria-label="Product catalog pages">
      <button class="button secondary" type="button" :disabled="pageIndex === 0 || loading" @click="previousPage">Previous page</button>
      <span aria-live="polite">Page {{ pageIndex + 1 }}<small>{{ publishedProducts.length }} of {{ resultTotal }}</small></span>
      <button class="button secondary" type="button" :disabled="!pageNextCursor || loading" @click="nextPage">Next page</button>
    </nav>
    <p v-if="loading" class="catalog-loading" role="status">Loading published product records…</p>
  </section>
</template>
