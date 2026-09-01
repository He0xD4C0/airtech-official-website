<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ArrowRight, Boxes, CircleAlert, Download, Eye, Filter, Search, ShieldCheck, SlidersHorizontal } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import type { ProductSummary } from '@/types/domain'

const query = ref('')

const demoProducts: ProductSummary[] = [
  { id: 'demo-product-001', model: 'DEMO-NOT-FOR-PUBLISH-001', family: 'Centrifugal fans', sourceState: 'conflict', publishState: 'draft', verifiedFields: 18, totalFields: 24 },
  { id: 'demo-product-002', model: 'DEMO-NOT-FOR-PUBLISH-002', family: 'Axial fans', sourceState: 'pending', publishState: 'draft', verifiedFields: 14, totalFields: 22, overrideExpiresAt: '2026-09-30' },
  { id: 'demo-product-003', model: 'DEMO-NOT-FOR-PUBLISH-003', family: 'Motors', sourceState: 'notImported', publishState: 'archived', verifiedFields: 0, totalFields: 20 },
]

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
      sourceState: product.specifications.some((spec) => spec.state === 'pendingVerification') ? 'pending' : 'synced',
      publishState: product.status,
      verifiedFields: product.specifications.filter((spec) => spec.state === 'verified').length,
      totalFields: product.specifications.length,
    })),
  }
}, {
  errorMessage: '无法读取产品记录。',
})
const loadError = computed(() => productPager.error.value ?? '')
const products = computed(() => mockApiEnabled ? demoProducts : productPager.items.value)
const filtered = computed(() => products.value.filter((product) => `${product.model} ${product.family}`.toLowerCase().includes(query.value.toLowerCase())))

onMounted(async () => {
  if (mockApiEnabled) return
  await productPager.first()
})

const sourceBadge = (state: ProductSummary['sourceState']) => ({
  synced: { label: '已同步', tone: 'success' as const },
  conflict: { label: '存在冲突', tone: 'danger' as const },
  pending: { label: '待校验', tone: 'warning' as const },
  notImported: { label: '未导入', tone: 'neutral' as const },
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

    <div v-if="loadError" class="demo-banner"><span>API 错误</span><p>{{ loadError }}</p></div>

    <section class="source-authority-banner">
      <div class="source-authority-banner__icon"><ShieldCheck :size="21" /></div>
      <div><strong>来源边界已启用</strong><p>网站字段可在后台编辑；Feishu-owned 字段的本地修正必须设置原因与到期日，且不会回写 Feishu。</p></div>
      <RouterLink to="/integrations/feishu">查看字段映射<ArrowRight :size="15" /></RouterLink>
    </section>

    <section class="family-grid" aria-label="产品家族">
      <article v-for="family in [
        ['Centrifugal fans', '离心风机'],
        ['Axial fans', '轴流风机'],
        ['Cross-flow fans', '横流风机'],
        ['Inline duct fans', '管道风机'],
        ['Motors', '电机'],
      ]" :key="family[0]">
        <span><Boxes :size="18" /></span><div><strong>{{ family[0] }}</strong><p>{{ family[1] }} · 型号数等待 Product Master</p></div><ArrowRight :size="16" />
      </article>
    </section>

    <section class="panel table-panel">
      <div class="table-toolbar">
        <label class="search-field"><Search :size="17" /><input v-model="query" placeholder="搜索稳定 ID、型号或家族" /></label>
        <div class="table-toolbar__filters"><Filter :size="16" /><select aria-label="产品家族"><option>全部家族</option><option>Centrifugal fans</option><option>Axial fans</option><option>Cross-flow fans</option><option>Inline duct fans</option><option>Motors</option></select><select aria-label="数据状态"><option>全部数据状态</option><option>存在冲突</option><option>待校验</option></select></div>
      </div>
      <div v-if="mockApiEnabled" class="safe-demo-label"><CircleAlert :size="15" /><span>以下为界面专用演示记录，不包含真实型号、规格、曲线、认证或可发布数据。</span></div>
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
        :can-previous="!mockApiEnabled && productPager.canPrevious.value"
        :can-next="!mockApiEnabled && productPager.canNext.value"
        :loading="productPager.loading.value"
        :label="mockApiEnabled ? '条开发演示记录' : '条数据库记录（搜索作用于当前页）'"
        @previous="productPager.previous"
        @next="productPager.next"
      />
    </section>
  </div>
</template>
