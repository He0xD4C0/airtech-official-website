<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import DataNotice from '@/components/common/DataNotice.vue'
import PqCurve from '@/components/product/PqCurve.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import ProductMediaGallery from '@/components/product/ProductMediaGallery.vue'
import PageSlotSections from './PageSlotSections.vue'
import { useCompareStore } from '@/stores/compare'
import type { PublicPageModel } from '@/types/content'
import { hasCompleteProductContext } from '@/lib/submissions'
import { publicMediaHref } from '@/types/projection'

const props = defineProps<{ page: PublicPageModel }>()
type ProductTab = 'overview' | 'performance'
const family = computed(() => props.page.productFamilies?.find((item) => item.slug === props.page.category))
const product = computed(() => props.page.publishedProduct)
const sourceAssets = computed(() => props.page.productAssets ?? [])
const verifiedCurve = computed(() => product.value?.performanceCurves.find((curve) => curve.state === 'verified'))
const tabs = computed<ProductTab[]>(() => [
  ...(product.value?.specifications.length ? ['overview' as const] : []),
  ...(verifiedCurve.value ? ['performance' as const] : []),
])
const activeTab = ref<ProductTab>(tabs.value[0] ?? 'overview')
const enhancedTabs = ref(false)
const compare = useCompareStore()
const added = computed(() => Boolean(product.value && compare.items.some((item) => item.id === product.value?.id)))
const productRfqHref = computed(() => hasCompleteProductContext(props.page.productContext) && product.value
    ? `/en/request-a-quote/product?${new URLSearchParams({ product: product.value.slug, family: product.value.family }).toString()}`
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
  compare.add({ id: product.value.id, slug: product.value.slug, label: product.value.title, family: family.value?.name ?? product.value.family, familyCode: product.value.family, dataState: 'published', publishedRevision: product.value.publishedRevision ?? undefined })
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

function assetHref(value: string): string {
  return publicMediaHref(value)
}

function fileType(name: string, mediaType: string): string {
  return name.split('.').at(-1)?.toUpperCase() || mediaType.split('/').at(-1)?.toUpperCase() || 'FILE'
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`
  return `${(bytes / 1024 ** 2).toFixed(1)} MiB`
}
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs">
      <div class="status-row"><span :class="['status', product ? 'published' : 'pending']">{{ product ? 'Published Product Master' : 'Published record unavailable' }}</span><span>Revision: {{ product?.publishedRevision ?? 'not published' }}</span></div>
    </PageHero>
    <ProductMediaGallery v-if="product?.mediaGallery.length" :items="product.mediaGallery" />
    <section class="section shell product-overview">
      <div>
        <DataNotice v-if="!product" title="Product data unavailable" :text="page.placeholderReason || 'No published Product Master record is available. No product values have been assumed.'" />
        <dl v-if="product" class="spec-summary">
          <div v-if="product.model"><dt>Exact model</dt><dd>{{ product.model }}</dd></div>
          <div v-if="family?.name"><dt>Fan form</dt><dd>{{ family.name }}</dd></div>
          <div v-if="product.motorTechnology"><dt>Motor technology</dt><dd>{{ product.motorTechnology }}</dd></div>
          <div v-if="product.stableId"><dt>Stable ID</dt><dd>{{ product.stableId }}</dd></div>
        </dl>
        <div class="button-row">
          <button v-if="product" class="button secondary" type="button" :disabled="added" @click="addProduct">{{ added ? 'Added to compare' : 'Add to compare' }}</button>
          <a v-if="productRfqHref" class="button" :href="productRfqHref">Request a quote for this product</a>
          <a v-else class="button" href="/en/request-a-quote/selection">Request selection support</a>
        </div>
      </div>
    </section>
    <section v-if="sourceAssets.length" class="section shell source-assets" aria-labelledby="source-assets-title">
      <div class="source-assets__heading"><div><p class="eyebrow">PRODUCT MASTER FILES</p><h2 id="source-assets-title">Source attachments</h2></div><p>Original files synchronized from the current published Feishu revision.</p></div>
      <div class="source-assets__grid">
        <article v-for="asset in sourceAssets" :key="asset.assetId" class="source-asset-card">
          <img v-if="asset.previewUrl" :src="assetHref(asset.previewUrl)" :alt="asset.originalName" loading="lazy" />
          <div v-else class="source-asset-card__type" aria-hidden="true">{{ fileType(asset.originalName, asset.mediaType) }}</div>
          <div class="source-asset-card__body"><strong>{{ asset.originalName }}</strong><span>{{ asset.usage }} · {{ formatBytes(asset.byteSize) }}</span><small>SHA-256 {{ asset.sha256.slice(0, 12) }}…</small><a class="button secondary" :href="assetHref(asset.downloadUrl)" download>Download original</a></div>
        </article>
      </div>
    </section>
    <section v-if="tabs.length" class="shell product-tabs">
      <div v-if="tabs.length > 1" class="tab-list" role="tablist" aria-label="Product information">
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
      <div v-if="product?.specifications.length" id="product-panel-overview" class="tab-panel" role="tabpanel" :aria-labelledby="tabs.length > 1 ? 'product-tab-overview' : undefined" :hidden="enhancedTabs && activeTab !== 'overview'"><h2>Structured specifications</h2><div class="table-scroll"><table><tbody><tr v-for="spec in product.specifications" :key="spec.key"><th scope="row">{{ spec.label }}</th><td>{{ spec.value === undefined || spec.value === null ? spec.state : displayValue(spec.value) }} {{ spec.unit || '' }}<small v-if="spec.operatingCondition"> · {{ spec.operatingCondition }}</small></td></tr></tbody></table></div></div>
      <div v-if="verifiedCurve" id="product-panel-performance" class="tab-panel" role="tabpanel" :aria-labelledby="tabs.length > 1 ? 'product-tab-performance' : undefined" :hidden="enhancedTabs && activeTab !== 'performance'"><h2>Airflow and pressure</h2><PqCurve :points="verifiedCurve.points" :airflow-unit="verifiedCurve.airflowUnit" :pressure-unit="verifiedCurve.pressureUnit" :conditions="curveConditions()" /></div>
    </section>
    <PageSlotSections :sections="page.sections" />
    <CallToAction
      v-if="page.primaryCta"
      :eyebrow="page.primaryCta.eyebrow"
      :title="page.primaryCta.title"
      :description="page.primaryCta.description"
      :href="page.primaryCta.href"
      :label="page.primaryCta.label"
    />
  </main>
</template>

<style scoped>
@layer components {
.source-assets { display: grid; gap: var(--space-5); }
.source-assets__heading { display: flex; align-items: end; justify-content: space-between; gap: var(--space-5); }
.source-assets__heading h2,.source-assets__heading p { margin: 0; }
.source-assets__grid { display: grid; grid-template-columns: repeat(auto-fit,minmax(min(100%,16rem),1fr)); gap: var(--space-4); }
.source-asset-card { overflow: hidden; border: 1px solid var(--color-border); border-radius: var(--airtek-radius-md); background: var(--color-surface); }
.source-asset-card > img,.source-asset-card__type { width: 100%; aspect-ratio: 16 / 10; object-fit: cover; }
.source-asset-card__type { display: grid; place-items: center; background: var(--color-surface-subtle); font: 700 1.5rem/1 monospace; letter-spacing: .08em; }
.source-asset-card__body { display: grid; gap: .55rem; padding: var(--space-4); }
.source-asset-card__body span,.source-asset-card__body small { color: var(--color-text-muted); overflow-wrap: anywhere; }
.source-asset-card__body .button { justify-self: start; }
@media (max-width: 720px) { .source-assets__heading { align-items: start; flex-direction: column; } }
}
</style>
