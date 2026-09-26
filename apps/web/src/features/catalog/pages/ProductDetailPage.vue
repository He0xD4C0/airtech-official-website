<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { PerformanceCurve, ProductSourceAsset, SpecValue } from '@airtek/contracts'
import PageHero from '@/shared/components/common/PageHero.vue'
import DataNotice from '@/shared/components/common/DataNotice.vue'
import PqCurve from '@/features/catalog/components/PqCurve.vue'
import { CallToAction } from '@/features/content'
import ProductMediaGallery from '@/features/catalog/components/ProductMediaGallery.vue'
import PageSlotSections from '@/shared/components/PageSlotSections.vue'
import { useCompareStore } from '@/features/compare'
import type { PublicPageModel } from '@/shared/types/content'
import { hasCompleteProductContext } from '@/features/conversion'
import { publicMediaHref } from '@/shared/types/projection'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
type ProductTab = 'overview' | 'performance'
const family = computed(() => props.page.productFamilies?.find((item) => item.slug === props.page.category))
const product = computed(() => props.page.publishedProduct)
const sourceAssets = computed(() => props.page.productAssets ?? [])
const verifiedCurves = computed(() => product.value?.performanceCurves.filter((curve) => curve.state === 'verified') ?? [])
const tabs = computed<ProductTab[]>(() => [
  ...(product.value?.specifications.length ? ['overview' as const] : []),
  ...(verifiedCurves.value.length ? ['performance' as const] : []),
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
  return t('product.sourceRecord')
}

const specificationGroups = computed(() => {
  const groups: Array<{ key: string; title: string; items: SpecValue[] }> = [
    { key: 'mechanical', title: t('product.mechanical'), items: [] },
    { key: 'electrical', title: t('product.electrical'), items: [] },
    { key: 'protection', title: t('product.protection'), items: [] },
    { key: 'packaging', title: t('product.packaging'), items: [] },
    { key: 'other', title: t('product.otherFacts'), items: [] },
  ]
  for (const spec of product.value?.specifications ?? []) {
    const group = ['diameter', 'productDimensions'].includes(spec.key) ? 'mechanical'
      : ['voltage', 'frequency', 'current', 'power', 'insulation'].includes(spec.key) ? 'electrical'
        : ['protection', 'certification', 'certifications'].includes(spec.key) ? 'protection'
          : ['packageDimensions', 'packageWeight', 'packaging'].includes(spec.key) ? 'packaging'
            : 'other'
    groups.find((entry) => entry.key === group)?.items.push(spec)
  }
  return groups.filter((group) => group.items.length)
})

const assetGroups = computed(() => {
  const definitions: Array<{ usage: ProductSourceAsset['usage']; title: string }> = [
    { usage: 'curve', title: t('product.performanceCurves') },
    { usage: 'drawing', title: t('product.drawings') },
    { usage: 'cad', title: t('product.cadFiles') },
    { usage: 'datasheet', title: t('product.dataSheets') },
    { usage: 'technicalDocument', title: t('product.technicalDocuments') },
  ]
  return definitions.map((definition) => ({
    ...definition,
    items: sourceAssets.value.filter((asset) => asset.usage === definition.usage),
  })).filter((group) => group.items.length)
})

function curveConditions(curve: PerformanceCurve) {
  return [
    curve.speedRpm ? `${curve.speedRpm} rpm` : '',
    curve.densityKgM3 ? `${curve.densityKgM3} kg/m³` : '',
    curve.voltage || '',
    curve.testMethod || '',
  ].filter(Boolean).join(' · ') || t('product.conditionsMissing')
}

function factState(state: SpecValue['state']): string {
  return ({
    verified: t('product.verified'), missing: t('common.notPublished'), notApplicable: t('product.notApplicable'),
    notTested: t('product.notTested'), confidential: t('product.confidential'), pendingVerification: t('product.pendingVerification'),
  })[state]
}

function assetHref(value: string): string {
  return publicMediaHref(value)
}

function fileType(name: string, mediaType: string): string {
  return name.split('.').at(-1)?.toUpperCase() || mediaType.split('/').at(-1)?.toUpperCase() || t('product.file')
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
      <div class="status-row" :lang="locale"><span :class="['status', product ? 'published' : 'pending']">{{ product ? t('product.publishedMaster') : t('product.unavailable') }}</span><span>{{ t('product.revision', { revision: product?.publishedRevision ?? t('common.notPublished') }) }}</span></div>
    </PageHero>
    <ProductMediaGallery v-if="product?.mediaGallery.length" :items="product.mediaGallery" />
    <section class="section shell product-overview" :lang="locale">
      <div>
        <DataNotice v-if="!product" :title="t('product.dataUnavailable')" :text="page.placeholderReason || t('product.noMaster')" />
        <dl v-if="product" class="spec-summary">
          <div v-if="product.model"><dt>{{ t('product.exactModel') }}</dt><dd lang="en">{{ product.model }}</dd></div>
          <div v-if="family?.name"><dt>{{ t('product.fanForm') }}</dt><dd lang="en">{{ family.name }}</dd></div>
          <div v-if="product.motorTechnology"><dt>{{ t('product.motorTechnology') }}</dt><dd lang="en">{{ product.motorTechnology }}</dd></div>
          <div v-if="product.stableId"><dt>{{ t('product.stableId') }}</dt><dd lang="en">{{ product.stableId }}</dd></div>
        </dl>
        <DataNotice v-if="product && !product.summary" :title="t('product.summaryMissing')" :text="t('product.summaryHelp')" />
        <div class="button-row">
          <button v-if="product" class="button secondary" type="button" :disabled="added" @click="addProduct">{{ added ? t('product.added') : t('product.add') }}</button>
          <a v-if="productRfqHref" class="button" :href="productRfqHref">{{ t('product.quote') }}</a>
          <a v-else class="button" href="/en/request-a-quote/selection">{{ t('product.selectionSupport') }}</a>
        </div>
      </div>
    </section>
    <section v-if="sourceAssets.length" class="section shell source-assets" :lang="locale" aria-labelledby="source-assets-title">
      <div class="source-assets__heading"><div><p class="eyebrow">{{ t('product.filesEyebrow') }}</p><h2 id="source-assets-title">{{ t('product.sourceAttachments') }}</h2></div><p>{{ t('product.filesDescription') }}</p></div>
      <div v-for="group in assetGroups" :key="group.usage" class="source-asset-group">
        <h3>{{ group.title }}</h3>
        <div class="source-assets__grid">
          <article v-for="asset in group.items" :key="asset.assetId" class="source-asset-card">
            <img v-if="asset.previewUrl && asset.mediaType.startsWith('image/')" :src="assetHref(asset.previewUrl)" :alt="asset.originalName" loading="lazy" />
            <div v-else class="source-asset-card__type" aria-hidden="true">{{ fileType(asset.originalName, asset.mediaType) }}</div>
            <div class="source-asset-card__body" lang="en"><strong>{{ asset.originalName }}</strong><span>{{ fileType(asset.originalName, asset.mediaType) }} · {{ formatBytes(asset.byteSize) }}</span><small>SHA-256 {{ asset.sha256.slice(0, 12) }}…</small><a class="button secondary" :lang="locale" :href="assetHref(asset.downloadUrl)" download>{{ t('common.downloadOriginal') }}</a></div>
          </article>
        </div>
      </div>
    </section>
    <section v-if="tabs.length" class="shell product-tabs" :lang="locale">
      <div v-if="tabs.length > 1" class="tab-list" role="tablist" :aria-label="t('product.productInformation')">
        <button
          v-for="tab in tabs"
          :id="`product-tab-${tab}`"
          :key="tab"
          type="button"
          role="tab"
          :aria-controls="`product-panel-${tab}`"
          :aria-selected="activeTab === tab"
          @click="selectTab(tab)"
        >{{ t(`product.${tab}`) }}</button>
      </div>
      <div v-if="product?.specifications.length" id="product-panel-overview" class="tab-panel" role="tabpanel" :aria-labelledby="tabs.length > 1 ? 'product-tab-overview' : undefined" :hidden="enhancedTabs && activeTab !== 'overview'">
        <h2>{{ t('product.specifications') }}</h2>
        <section v-for="group in specificationGroups" :key="group.key" class="specification-group"><h3>{{ group.title }}</h3><div class="table-scroll"><table><tbody><tr v-for="spec in group.items" :key="spec.key"><th scope="row" lang="en">{{ spec.label }}</th><td><span :lang="spec.value === undefined || spec.value === null ? locale : 'en'">{{ spec.value === undefined || spec.value === null ? factState(spec.state) : displayValue(spec.value) }} {{ spec.unit || '' }}</span><small>{{ factState(spec.state) }}<template v-if="spec.operatingCondition"> · <span lang="en">{{ spec.operatingCondition }}</span></template></small></td></tr></tbody></table></div></section>
      </div>
      <div v-if="verifiedCurves.length" id="product-panel-performance" class="tab-panel" role="tabpanel" :aria-labelledby="tabs.length > 1 ? 'product-tab-performance' : undefined" :hidden="enhancedTabs && activeTab !== 'performance'">
        <h2>{{ t('product.airflowPressure') }}</h2>
        <section v-for="(curve, index) in verifiedCurves" :key="`${curve.sourceReference}:${index}`" class="performance-curve"><h3>{{ t('product.verifiedCurve', { number: index + 1 }) }}</h3><PqCurve :points="curve.points" :airflow-unit="curve.airflowUnit" :pressure-unit="curve.pressureUnit" :conditions="curveConditions(curve)" /></section>
      </div>
    </section>
    <section v-if="page.relatedEntries?.length" class="section shell" :lang="locale" aria-labelledby="related-content-title"><p class="eyebrow">{{ t('product.relatedEyebrow') }}</p><h2 id="related-content-title">{{ t('product.related') }}</h2><div class="card-grid two" lang="en"><article v-for="entry in page.relatedEntries" :key="entry.href" class="card"><p class="eyebrow">{{ entry.eyebrow }}</p><h3><a :href="entry.href">{{ entry.title }}</a></h3><p>{{ entry.summary || t('search.summaryMissing') }}</p></article></div></section>
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
.source-asset-group { display: grid; gap: var(--space-3); }
.source-asset-group > h3,.specification-group > h3,.performance-curve > h3 { margin: 0; }
.specification-group,.performance-curve { margin-block: var(--space-5); }
.specification-group td { display: grid; gap: .25rem; }
.specification-group td small { color: var(--color-text-muted); }
.source-asset-card { overflow: hidden; border: 1px solid var(--color-border); border-radius: var(--airtek-radius-md); background: var(--color-surface); }
.source-asset-card > img,.source-asset-card__type { width: 100%; aspect-ratio: 16 / 10; object-fit: cover; }
.source-asset-card__type { display: grid; place-items: center; background: var(--color-surface-subtle); font: 700 1.5rem/1 monospace; letter-spacing: .08em; }
.source-asset-card__body { display: grid; gap: .55rem; padding: var(--space-4); }
.source-asset-card__body span,.source-asset-card__body small { color: var(--color-text-muted); overflow-wrap: anywhere; }
.source-asset-card__body .button { justify-self: start; }
@media (max-width: 720px) { .source-assets__heading { align-items: start; flex-direction: column; } }
}
</style>
