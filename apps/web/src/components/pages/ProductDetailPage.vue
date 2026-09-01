<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { productFamilies } from '@/content/catalog'
import PageHero from '@/components/common/PageHero.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import PqCurve from '@/components/product/PqCurve.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import { useCompareStore } from '@/stores/compare'
import type { PublicPageModel } from '@/types/content'
import { hasCompleteProductContext } from '@/lib/submissions'

const props = defineProps<{ page: PublicPageModel }>()
type ProductTab = 'overview' | 'performance' | 'resources'
const tabs: ProductTab[] = ['overview', 'performance', 'resources']
const family = computed(() => productFamilies.find((item) => item.slug === props.page.category))
const product = computed(() => props.page.publishedProduct)
const activeTab = ref<ProductTab>('overview')
const enhancedTabs = ref(false)
const compare = useCompareStore()
const added = computed(() => Boolean(product.value && compare.items.some((item) => item.id === product.value?.id)))
const verifiedCurve = computed(() => product.value?.performanceCurves.find((curve) => curve.state === 'verified'))
const productRfqHref = computed(() => hasCompleteProductContext(props.page.productContext) && product.value
  ? `/en/request-a-quote/product?product=${encodeURIComponent(product.value.slug)}`
  : undefined)

onMounted(() => {
  // Keep every technical section in the server-rendered, no-JavaScript page.
  // Once hydrated, the same markup becomes a compact tab interface.
  enhancedTabs.value = true
})

function selectTab(tab: ProductTab): void {
  activeTab.value = tab
}

function addProduct() {
  if (!product.value) return
  compare.hydrate()
  compare.add({ id: product.value.id, slug: product.value.slug, label: product.value.title, family: family.value?.name ?? product.value.family, dataState: 'published', publishedRevision: product.value.publishedRevision ?? undefined })
}

function displayValue(value: unknown) {
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return String(value)
  return 'See controlled source record'
}

function curveConditions() {
  const curve = verifiedCurve.value
  if (!curve) return undefined
  return [
    curve.speedRpm ? `${curve.speedRpm} rpm` : '',
    curve.densityKgM3 ? `${curve.densityKgM3} kg/m³` : '',
    curve.voltage || '',
    curve.testMethod || '',
  ].filter(Boolean).join(' · ')
}
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs">
      <div class="status-row"><span :class="['status', product ? 'published' : 'pending']">{{ product ? 'Published Product Master' : 'Published record unavailable' }}</span><span>Revision: {{ product?.publishedRevision ?? 'not published' }}</span></div>
    </PageHero>
    <section class="section shell product-overview">
      <div class="product-placeholder" aria-label="Product image pending approval"><span>AIRTEKPOWER</span><strong>Approved product image pending</strong></div>
      <div>
        <DataNotice v-if="!product" title="Product data unavailable" :text="page.placeholderReason || 'No published Product Master record is available. No product values have been assumed.'" />
        <dl class="spec-summary">
          <div><dt>Exact model</dt><dd>{{ product?.model ?? 'Not available' }}</dd></div><div><dt>Fan form</dt><dd>{{ family?.form ?? product?.family ?? 'Not available' }}</dd></div>
          <div><dt>Motor technology</dt><dd>{{ product?.motorTechnology ?? 'Not published' }}</dd></div><div><dt>Stable ID</dt><dd>{{ product?.stableId ?? 'Not available' }}</dd></div>
        </dl>
        <div class="button-row">
          <button v-if="product" class="button secondary" type="button" :disabled="added" @click="addProduct">{{ added ? 'Added to compare' : 'Add to compare' }}</button>
          <a v-if="productRfqHref" class="button" :href="productRfqHref">Request a quote for this product</a>
          <a v-else class="button" href="/en/request-a-quote/selection">Request selection support</a>
        </div>
      </div>
    </section>
    <section class="shell product-tabs">
      <div class="tab-list" role="tablist" aria-label="Product information">
        <button
          v-for="tab in tabs"
          :id="`product-tab-${tab}`"
          :key="tab"
          type="button"
          role="tab"
          :aria-controls="`product-panel-${tab}`"
          :aria-selected="activeTab === tab"
          @click="selectTab(tab)"
        >{{ tab }}</button>
      </div>
      <div id="product-panel-overview" class="tab-panel" role="tabpanel" aria-labelledby="product-tab-overview" :hidden="enhancedTabs && activeTab !== 'overview'"><h2>Structured specifications</h2><div class="table-scroll"><table><tbody><tr v-if="!product?.specifications.length"><th scope="row">Data status</th><td>No published specifications are available.</td></tr><tr v-for="spec in product?.specifications" :key="spec.key"><th scope="row">{{ spec.label }}</th><td>{{ spec.value === undefined || spec.value === null ? spec.state : displayValue(spec.value) }} {{ spec.unit || '' }}<small v-if="spec.operatingCondition"> · {{ spec.operatingCondition }}</small></td></tr></tbody></table></div></div>
      <div id="product-panel-performance" class="tab-panel" role="tabpanel" aria-labelledby="product-tab-performance" :hidden="enhancedTabs && activeTab !== 'performance'"><h2>Airflow and pressure</h2><PqCurve :points="verifiedCurve?.points" :airflow-unit="verifiedCurve?.airflowUnit" :pressure-unit="verifiedCurve?.pressureUnit" :conditions="curveConditions()" /></div>
      <div id="product-panel-resources" class="tab-panel" role="tabpanel" aria-labelledby="product-tab-resources" :hidden="enhancedTabs && activeTab !== 'resources'"><h2>Controlled resources</h2><div class="empty-state"><h3>No approved downloads published</h3><p>Datasheets, CAD, manuals and certificates appear only with an applicable model, revision and controlled file.</p><a class="text-link" href="/en/resources/downloads">Visit Downloads →</a></div></div>
    </section>
    <CallToAction title="Need an exact operating-point review?" description="Start with verified context rather than assumptions from an unfinished product record." href="/en/request-a-quote/selection" label="Request selection support" />
  </main>
</template>
