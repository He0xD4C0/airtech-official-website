<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Product, ProductFamily as ProductFamilyValue } from '@airtek/contracts'
import { productFamilies } from '@/content/catalog'
import SectionHeading from '@/components/common/SectionHeading.vue'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { getPublishedProducts } from '@/lib/api'
import { PUBLIC_PRODUCT_PAGE_SIZE } from '@/lib/productPagination'
import { useCompareStore } from '@/stores/compare'

const props = withDefaults(defineProps<{ category?: string; products: Product[]; nextCursor?: string | null }>(), {
  nextCursor: null,
})
const query = ref('')
const selectedForm = ref(props.category ?? 'all')
const selectedMotor = ref('all')
const view = ref<'cards' | 'table'>('cards')
const pageProducts = ref<Product[]>([...props.products])
const pageNextCursor = ref<string | null>(props.nextCursor ?? null)
const cursorHistory = ref<Array<string | null>>([null])
const pageIndex = ref(0)
const loading = ref(false)
const pageError = ref('')
const observedMotorTechnologies = ref(new Set(
  props.products.flatMap((product) => product.motorTechnology?.trim() ? [product.motorTechnology.trim()] : []),
))
let requestGeneration = 0
const compare = useCompareStore()

const familySlugs: Record<ProductFamilyValue, string> = {
  centrifugal: 'centrifugal',
  axial: 'axial',
  crossFlow: 'cross-flow',
  inlineDuct: 'inline-duct',
  motors: 'motors',
}

const familyNames = new Map(productFamilies.map((family) => [family.slug, family.name]))
const familyValues: Record<string, ProductFamilyValue> = {
  centrifugal: 'centrifugal',
  axial: 'axial',
  'cross-flow': 'crossFlow',
  'inline-duct': 'inlineDuct',
  motors: 'motors',
}
const motorTechnologies = computed(() => [...observedMotorTechnologies.value].sort())

watch(
  [() => props.products, () => props.nextCursor, () => props.category],
  () => {
    requestGeneration += 1
    pageProducts.value = [...props.products]
    pageNextCursor.value = props.nextCursor ?? null
    cursorHistory.value = [null]
    pageIndex.value = 0
    selectedForm.value = props.category ?? 'all'
    selectedMotor.value = 'all'
    query.value = ''
    loading.value = false
    pageError.value = ''
    rememberMotorTechnologies(props.products, true)
  },
)

function rememberMotorTechnologies(products: Product[], replace = false): void {
  const values = replace ? new Set<string>() : new Set(observedMotorTechnologies.value)
  for (const product of products) {
    const technology = product.motorTechnology?.trim()
    if (technology) values.add(technology)
  }
  observedMotorTechnologies.value = values
}

function familySlug(product: Product): string {
  return familySlugs[product.family]
}

function familyName(product: Product): string {
  return familyNames.get(familySlug(product)) ?? product.family
}

function productHref(product: Product): string {
  return `/en/products/${familySlug(product)}/${product.slug}`
}

const publishedProducts = computed(() => pageProducts.value.filter((product) => (
  product.status === 'published'
  && typeof product.publishedRevision === 'number'
  && product.locale === 'en'
)))

const results = computed(() => publishedProducts.value.filter((product) => {
  const form = familySlug(product)
  const matchesForm = selectedForm.value === 'all' || form === selectedForm.value
  const matchesMotor = selectedMotor.value === 'all' || product.motorTechnology === selectedMotor.value
  const haystack = [product.title, product.model, product.stableId, product.subtype, product.motorTechnology, product.summary]
    .filter((value): value is string => typeof value === 'string')
    .join(' ')
    .toLowerCase()
  return matchesForm && matchesMotor && haystack.includes(query.value.trim().toLowerCase())
}))

function isCompared(product: Product): boolean {
  return compare.items.some((item) => item.id === product.id)
}

function addToCompare(product: Product): void {
  compare.hydrate()
  compare.add({ id: product.id, slug: product.slug, label: product.title, family: familyName(product), dataState: 'published', publishedRevision: product.publishedRevision ?? undefined })
}

function trackFilter(filterName: string, filterValue: string): void {
  void trackAnalyticsEvent('filterApplied', {
    filterName,
    filterValue,
    resultCount: results.value.length,
  })
}

function productListQuery(cursor: string | null) {
  const family = selectedForm.value === 'all' ? undefined : familyValues[selectedForm.value]
  return {
    limit: PUBLIC_PRODUCT_PAGE_SIZE,
    ...(family ? { family } : {}),
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
    rememberMotorTechnologies(page.items)
    return true
  } catch (cause) {
    if (generation === requestGeneration) {
      pageError.value = cause instanceof Error ? cause.message : 'The next catalog page could not be loaded.'
    }
    return false
  } finally {
    if (generation === requestGeneration) loading.value = false
  }
}

async function resetPagination(): Promise<void> {
  // A cursor is bound to its filters, so no previous or next cursor may survive
  // a filter change even when the replacement request fails.
  cursorHistory.value = [null]
  pageIndex.value = 0
  pageNextCursor.value = null
  await loadPage(null)
}

async function trackQueryFilter(): Promise<void> {
  await resetPagination()
  trackFilter('catalogSearch', query.value.trim() ? 'queryPresent' : 'all')
}

async function trackFamilyFilter(): Promise<void> {
  await resetPagination()
  trackFilter('family', selectedForm.value)
}

async function trackMotorFilter(): Promise<void> {
  await resetPagination()
  // Product Master labels are rendered in the UI, but their source text is not copied into analytics.
  trackFilter('motorTechnology', selectedMotor.value === 'all' ? 'all' : 'selected')
}

async function clearFilters(): Promise<void> {
  query.value = ''
  selectedForm.value = props.category ?? 'all'
  selectedMotor.value = 'all'
  await resetPagination()
  trackFilter('catalogFilters', 'reset')
}

function trackPageChange(direction: 'next' | 'previous'): void {
  void trackAnalyticsEvent('filterApplied', {
    filterName: 'catalogPagination',
    filterValue: direction,
    resultCount: pageProducts.value.length,
  })
}

async function nextPage(): Promise<void> {
  const cursor = pageNextCursor.value
  if (!cursor || loading.value) return
  if (await loadPage(cursor)) {
    cursorHistory.value = [...cursorHistory.value.slice(0, pageIndex.value + 1), cursor]
    pageIndex.value += 1
    trackPageChange('next')
  }
}

async function previousPage(): Promise<void> {
  if (pageIndex.value === 0 || loading.value) return
  const targetIndex = pageIndex.value - 1
  const cursor = cursorHistory.value[targetIndex] ?? null
  if (await loadPage(cursor)) {
    pageIndex.value = targetIndex
    trackPageChange('previous')
  }
}

</script>

<template>
  <section class="section shell">
    <SectionHeading
      eyebrow="Published catalog"
      title="Find a published product record"
      description="Search only the current published Product Master projection. Model identity, family and revision stay attached to each result."
    />
    <div class="filter-panel product-filter-panel" aria-label="Product filters" :aria-busy="loading">
      <label>
        <span>Search this product page</span>
        <input v-model="query" type="search" placeholder="Search this page by title, model or stable ID" @change="trackQueryFilter">
      </label>
      <label>
        <span>Fan form</span>
        <select v-model="selectedForm" @change="trackFamilyFilter">
          <option value="all">All product families</option>
          <option v-for="family in productFamilies" :key="family.id" :value="family.slug">{{ family.name }}</option>
        </select>
      </label>
      <label>
        <span>Motor technology</span>
        <select v-model="selectedMotor" @change="trackMotorFilter">
          <option value="all">All published technologies</option>
          <option v-for="technology in motorTechnologies" :key="technology" :value="technology">{{ technology }}</option>
        </select>
      </label>
      <p class="result-count" role="status">{{ results.length }} matching {{ results.length === 1 ? 'record' : 'records' }} on page {{ pageIndex + 1 }}</p>
      <p class="pagination-scope">Counts describe the current API page, not the total catalog.</p>
    </div>

    <div v-if="pageError" class="data-notice catalog-page-error" role="alert">
      <div><strong>Catalog page unavailable</strong><p>{{ pageError }}</p></div>
      <button class="button secondary" type="button" :disabled="loading" @click="resetPagination">Reload first page</button>
    </div>

    <div v-if="publishedProducts.length" class="catalog-toolbar" aria-label="Catalog view">
      <span>View</span>
      <button type="button" :aria-pressed="view === 'cards'" @click="view = 'cards'">Cards</button>
      <button type="button" :aria-pressed="view === 'table'" @click="view = 'table'">Table</button>
    </div>

    <div v-if="results.length && view === 'cards'" class="card-grid product-grid">
      <article v-for="product in results" :key="product.id" class="card product-card">
        <div class="product-card-visual" aria-hidden="true">
          <span>{{ familyName(product).charAt(0) }}</span>
        </div>
        <div class="product-card-body">
          <p class="eyebrow">{{ familyName(product) }}</p>
          <h2><a :href="productHref(product)">{{ product.title }}</a></h2>
          <dl class="product-card-meta">
            <div><dt>Model</dt><dd>{{ product.model || 'Not published' }}</dd></div>
            <div><dt>Stable ID</dt><dd>{{ product.stableId }}</dd></div>
            <div><dt>Revision</dt><dd>{{ product.publishedRevision }}</dd></div>
          </dl>
          <p>{{ product.summary || 'No published summary is available for this record.' }}</p>
          <ul v-if="product.subtype || product.motorTechnology" class="tag-list" aria-label="Published facets">
            <li v-if="product.subtype">{{ product.subtype }}</li>
            <li v-if="product.motorTechnology">{{ product.motorTechnology }}</li>
          </ul>
          <div class="product-card-actions">
            <a class="text-link" :href="productHref(product)">View product <span aria-hidden="true">→</span></a>
            <button class="button secondary" type="button" :disabled="isCompared(product)" @click="addToCompare(product)">{{ isCompared(product) ? 'Added' : 'Compare' }}</button>
          </div>
        </div>
      </article>
    </div>

    <div v-else-if="results.length" class="table-scroll published-products-table">
      <table>
        <caption>{{ results.length }} published product records</caption>
        <thead><tr><th scope="col">Product</th><th scope="col">Model</th><th scope="col">Family</th><th scope="col">Motor technology</th><th scope="col">Revision</th></tr></thead>
        <tbody>
          <tr v-for="product in results" :key="product.id">
            <th scope="row"><a :href="productHref(product)">{{ product.title }}</a><small>{{ product.stableId }}</small></th>
            <td>{{ product.model || 'Not published' }}</td>
            <td>{{ familyName(product) }}</td>
            <td>{{ product.motorTechnology || 'Not published' }}</td>
            <td>{{ product.publishedRevision }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-else-if="!publishedProducts.length" class="empty-state large">
      <p class="eyebrow">No published records</p>
      <h2>No validated products are available in this catalog view.</h2>
      <p>No model, performance value or compatibility claim has been inferred.</p>
      <a class="button" href="/en/request-a-quote/selection">Request selection support</a>
    </div>
    <div v-else class="empty-state">
      <h2>No matching published product</h2>
      <p>Try a broader term or remove a selected facet.</p>
      <button class="button secondary" type="button" @click="clearFilters">Clear filters</button>
    </div>

    <nav v-if="pageIndex > 0 || pageNextCursor" class="catalog-pagination" aria-label="Product catalog pages">
      <button class="button secondary" type="button" :disabled="pageIndex === 0 || loading" @click="previousPage">Previous page</button>
      <span aria-live="polite">Page {{ pageIndex + 1 }}<small>Current page only</small></span>
      <button class="button secondary" type="button" :disabled="!pageNextCursor || loading" @click="nextPage">Next page</button>
    </nav>
    <p v-if="loading" class="catalog-loading" role="status">Loading published product records…</p>
  </section>
</template>
