<script setup lang="ts">
import { adminProductApi } from '@/features/catalog/services/adminProductApi'
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { ProductFamily, PublicationStatus } from '@airtek/contracts'
import { ArrowRight, Boxes, Eye, Filter, Search, ShieldCheck, SlidersHorizontal } from 'lucide-vue-next'
import CursorPaginationControls from '@/shared/components/CursorPaginationControls.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import StatusBadge from '@/shared/components/StatusBadge.vue'
import { useCursorPagination } from '@/shared/composables/useCursorPagination'

import type { ProductSummary } from '@/shared/types/domain'

const route = useRoute()
const router = useRouter()
const query = ref('')
const family = ref<ProductFamily | ''>('')
const status = ref<PublicationStatus | ''>('')
const dataState = ref<'verified' | 'pending' | ''>('')
const total = ref(0)
const familyCounts = ref<Array<{ value: string; count: number }>>([])
let searchTimer: ReturnType<typeof setTimeout> | undefined

const familyNames: Record<ProductFamily, string> = {
  centrifugal: 'Centrifugal fans', axial: 'Axial fans', crossFlow: 'Cross-flow fans',
  inlineDuct: 'Inline duct fans', motors: 'Motors',
}

const productPager = useCursorPagination(async (pagination) => {
  const page = await adminProductApi.listProducts({
    ...pagination, q: query.value.trim() || undefined, family: family.value || undefined,
    status: status.value || undefined, dataState: dataState.value || undefined,
  })
  total.value = page.total
  familyCounts.value = page.familyCounts
  return {
    ...page,
    items: page.items.map((product): ProductSummary => ({
      id: product.id, model: product.model || product.stableId, family: familyNames[product.family],
      sourceState: product.specifications.some((spec) => spec.state === 'pendingVerification') ? 'pending' : 'loaded',
      publishState: product.status,
      verifiedFields: product.specifications.filter((spec) => spec.state === 'verified').length,
      totalFields: product.specifications.length,
    })),
  }
}, { errorMessage: '无法读取产品记录。' })

const products = computed(() => productPager.items.value)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (productPager.loading.value && !products.value.length) return 'loading'
  if (productPager.errorStatus.value === 403) return 'forbidden'
  if (productPager.error.value) return 'error'
  return products.value.length ? 'ready' : 'empty'
})

function routeString(value: unknown): string { return typeof value === 'string' ? value : '' }
function controlled<T extends string>(value: unknown, allowed: readonly T[]): T | '' {
  const candidate = routeString(value) as T
  return allowed.includes(candidate) ? candidate : ''
}

async function syncFromUrl(): Promise<void> {
  query.value = routeString(route.query.q)
  family.value = controlled(route.query.family, Object.keys(familyNames) as ProductFamily[])
  status.value = controlled(route.query.status, ['draft', 'scheduled', 'published', 'archived'] as const)
  dataState.value = controlled(route.query.dataState, ['verified', 'pending'] as const)
  await productPager.first()
}

function replaceFilters(): void {
  void router.replace({ query: {
    ...(query.value.trim() ? { q: query.value.trim() } : {}), ...(family.value ? { family: family.value } : {}),
    ...(status.value ? { status: status.value } : {}), ...(dataState.value ? { dataState: dataState.value } : {}),
  } })
}

watch(() => route.fullPath, () => void syncFromUrl(), { immediate: true })
watch([family, status, dataState], replaceFilters)
watch(query, () => {
  if (searchTimer !== undefined) clearTimeout(searchTimer)
  searchTimer = setTimeout(replaceFilters, 300)
})
onBeforeUnmount(() => { if (searchTimer !== undefined) clearTimeout(searchTimer) })

const sourceBadge = (value: ProductSummary['sourceState']) => ({
  loaded: { label: '记录已载入', tone: 'info' as const }, pending: { label: '待校验', tone: 'warning' as const },
})[value]
const publishBadge = (value: ProductSummary['publishState']) => ({
  draft: { label: '草稿', tone: 'neutral' as const }, scheduled: { label: '计划发布', tone: 'info' as const },
  published: { label: '已发布', tone: 'success' as const }, archived: { label: '已归档', tone: 'warning' as const },
})[value]
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="PRODUCT MASTER" title="产品中心" description="筛选、总数与家族分布均由服务端基于完整产品集合计算。"><template #actions><RouterLink class="button button--primary" to="/integrations/feishu"><SlidersHorizontal :size="16" />处理同步</RouterLink></template></PageHeader>
    <section class="source-authority-banner"><div class="source-authority-banner__icon"><ShieldCheck :size="21" /></div><div><strong>来源边界已启用</strong><p>Feishu-owned 字段的本地修正必须具备证据、原因与到期日。</p></div><RouterLink to="/integrations/feishu">查看字段映射<ArrowRight :size="15" /></RouterLink></section>
    <section class="family-grid" aria-label="产品家族全局计数"><article v-for="item in familyCounts" :key="item.value"><span><Boxes :size="18" /></span><div><strong>{{ familyNames[item.value as ProductFamily] || item.value }}</strong><p>全部匹配记录 {{ item.count }} 个型号</p></div></article></section>
    <section class="panel table-panel">
      <div class="table-toolbar"><label class="search-field"><Search :size="17" /><input v-model="query" placeholder="搜索稳定 ID、型号或家族" /></label><div class="table-toolbar__filters"><Filter :size="16" /><select v-model="family" aria-label="产品家族"><option value="">全部家族</option><option v-for="(label, value) in familyNames" :key="value" :value="value">{{ label }}</option></select><select v-model="dataState" aria-label="数据状态"><option value="">全部数据状态</option><option value="verified">已校验</option><option value="pending">待校验</option></select><select v-model="status" aria-label="发布状态"><option value="">全部发布状态</option><option value="draft">草稿</option><option value="scheduled">计划发布</option><option value="published">已发布</option><option value="archived">已归档</option></select></div></div>
      <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '没有匹配的产品记录' : state === 'error' ? productPager.error.value || '' : ''" @retry="productPager.refresh" />
      <template v-else><div class="data-table-wrap"><table class="data-table product-table"><thead><tr><th>产品</th><th>产品家族</th><th>来源状态</th><th>字段完整度</th><th>发布状态</th><th aria-label="操作"></th></tr></thead><tbody><tr v-for="product in products" :key="product.id"><td><strong>{{ product.model }}</strong><span>{{ product.id }}</span></td><td>{{ product.family }}</td><td><StatusBadge v-bind="sourceBadge(product.sourceState)" /></td><td><div class="completion"><span><i :style="{ width: `${product.totalFields ? Math.round(product.verifiedFields / product.totalFields * 100) : 0}%` }"></i></span><small>{{ product.verifiedFields }}/{{ product.totalFields }}</small></div></td><td><StatusBadge v-bind="publishBadge(product.publishState)" /></td><td><RouterLink class="icon-button" :to="`/products/${encodeURIComponent(product.id)}`" :aria-label="`查看产品 ${product.model}`"><Eye :size="18" /></RouterLink></td></tr></tbody></table></div><CursorPaginationControls :item-count="products.length" :page-number="productPager.pageNumber.value" :can-previous="productPager.canPrevious.value" :can-next="productPager.canNext.value" :loading="productPager.loading.value" :label="`条记录；服务端匹配总数 ${total}`" @previous="productPager.previous" @next="productPager.next" /></template>
    </section>
  </div>
</template>
