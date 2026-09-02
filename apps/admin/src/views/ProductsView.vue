<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ArrowRight, Boxes, Download, Eye, Filter, Search, ShieldCheck, SlidersHorizontal } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi } from '@/services/adminApi'
import type { ProductSummary } from '@/types/domain'

const query = ref('')
const familyFilter = ref('all')

const familyNames = {
  centrifugal: 'Centrifugal fans',
  axial: 'Axial fans',
  crossFlow: 'Cross-flow fans',
  inlineDuct: 'Inline duct fans',
  motors: 'Motors',
} as const

const productPager = useCursorPagination(async (pagination) => {
  const page = await adminApi.listProducts(pagination)
  return {
    ...page,
    items: page.items.map((product): ProductSummary => ({
      id: product.id,
      model: product.model || product.stableId,
      family: familyNames[product.family],
      sourceState: product.specifications.some((spec) => spec.state === 'pendingVerification') ? 'pending' : 'loaded',
      publishState: product.status,
      verifiedFields: product.specifications.filter((spec) => spec.state === 'verified').length,
      totalFields: product.specifications.length,
    })),
  }
}, {
  errorMessage: '无法读取产品记录。',
})
const loadError = computed(() => productPager.error.value ?? '')
const products = computed(() => productPager.items.value)
const filtered = computed(() => products.value.filter((product) => {
  const matchesQuery = `${product.model} ${product.family}`.toLowerCase().includes(query.value.toLowerCase())
  return matchesQuery && (familyFilter.value === 'all' || product.family === familyFilter.value)
}))
const familySummaries = computed(() => Array.from(
  products.value.reduce((counts, product) => counts.set(product.family, (counts.get(product.family) ?? 0) + 1), new Map<string, number>()),
  ([name, count]) => ({ name, count }),
))
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (productPager.loading.value && !products.value.length) return 'loading'
  if (productPager.errorStatus.value === 403) return 'forbidden'
  if (productPager.error.value) return 'error'
  return products.value.length ? 'ready' : 'empty'
})

onMounted(async () => {
  await productPager.first()
})

const sourceBadge = (state: ProductSummary['sourceState']) => ({
  loaded: { label: '记录已载入', tone: 'info' as const },
  pending: { label: '待校验', tone: 'warning' as const },
})[state]

const publishBadge = (state: ProductSummary['publishState']) => ({
  draft: { label: '草稿', tone: 'neutral' as const },
  scheduled: { label: '计划发布', tone: 'info' as const },
  published: { label: '已发布', tone: 'success' as const },
  archived: { label: '已归档', tone: 'warning' as const },
})[state]
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="PRODUCT MASTER" title="产品中心" description="Feishu 是产品主数据唯一权威来源；后台只发布通过校验的产品 revision。">
      <template #actions>
        <button class="button button--secondary" type="button"><Download :size="16" />导出校验报告</button>
        <RouterLink class="button button--primary" to="/integrations/feishu"><SlidersHorizontal :size="16" />处理同步</RouterLink>
      </template>
    </PageHeader>

    <section class="source-authority-banner">
      <div class="source-authority-banner__icon"><ShieldCheck :size="21" /></div>
      <div><strong>来源边界已启用</strong><p>网站字段可在后台编辑；Feishu-owned 字段的本地修正必须设置原因与到期日，且不会回写 Feishu。</p></div>
      <RouterLink to="/integrations/feishu">查看字段映射<ArrowRight :size="15" /></RouterLink>
    </section>

    <section class="family-grid" aria-label="产品家族">
      <article v-for="family in familySummaries" :key="family.name">
        <span><Boxes :size="18" /></span><div><strong>{{ family.name }}</strong><p>当前 API 页 {{ family.count }} 个型号</p></div><ArrowRight :size="16" />
      </article>
    </section>

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? '数据库中暂无产品记录' : state === 'error' ? loadError : ''"
      @retry="productPager.refresh"
    />

    <section v-else class="panel table-panel">
      <div class="table-toolbar">
        <label class="search-field"><Search :size="17" /><input v-model="query" placeholder="搜索稳定 ID、型号或家族" /></label>
        <div class="table-toolbar__filters"><Filter :size="16" /><select v-model="familyFilter" aria-label="产品家族"><option value="all">全部家族</option><option v-for="family in familySummaries" :key="family.name" :value="family.name">{{ family.name }}</option></select><select aria-label="数据状态"><option>全部数据状态</option><option>存在冲突</option><option>待校验</option></select></div>
      </div>
      <div class="data-table-wrap">
        <table class="data-table product-table">
          <thead><tr><th>产品</th><th>产品家族</th><th>来源状态</th><th>字段完整度</th><th>发布状态</th><th><span class="sr-only">操作</span></th></tr></thead>
          <tbody>
            <tr v-for="product in filtered" :key="product.id">
              <td><strong>{{ product.model }}</strong><span>{{ product.id }}<em v-if="product.overrideExpiresAt">临时覆盖至 {{ product.overrideExpiresAt }}</em></span></td>
              <td>{{ product.family }}</td>
              <td><StatusBadge v-bind="sourceBadge(product.sourceState)" /></td>
              <td><div class="completion"><span><i :style="{ width: `${product.totalFields ? Math.round(product.verifiedFields / product.totalFields * 100) : 0}%` }"></i></span><small>{{ product.verifiedFields }}/{{ product.totalFields }}</small></div></td>
              <td><StatusBadge v-bind="publishBadge(product.publishState)" /></td>
              <td><RouterLink class="icon-button" :to="`/products/${encodeURIComponent(product.id)}`" :aria-label="`查看产品 ${product.model}`"><Eye :size="18" /></RouterLink></td>
            </tr>
          </tbody>
        </table>
      </div>
      <CursorPaginationControls
        :item-count="filtered.length"
        :page-number="productPager.pageNumber.value"
        :can-previous="productPager.canPrevious.value"
        :can-next="productPager.canNext.value"
        :loading="productPager.loading.value"
        label="条数据库记录（搜索作用于当前页）"
        @previous="productPager.previous"
        @next="productPager.next"
      />
    </section>
  </div>
</template>
