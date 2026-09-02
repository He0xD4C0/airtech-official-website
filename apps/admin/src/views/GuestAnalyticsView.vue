<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { BarChart3, ExternalLink, RefreshCw, ShieldCheck } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type GuestSourceDaily, type GuestVisitAggregate } from '@/services/adminApi'

const props = defineProps<{ mode: 'visits' | 'sources' }>()
const visitPager = useCursorPagination<GuestVisitAggregate>(
  (pagination) => adminApi.listGuestVisits(pagination),
  { errorMessage: '访问趋势读取失败。' },
)
const sourcePager = useCursorPagination<GuestSourceDaily>(
  (pagination) => adminApi.listGuestSources(pagination),
  { errorMessage: '来源聚合读取失败。' },
)
const visits = computed(() => visitPager.items.value)
const sources = computed(() => sourcePager.items.value)
const maxVisits = computed(() => Math.max(1, ...sources.value.map((item) => item.visits)))
const activePager = computed(() => props.mode === 'visits' ? visitPager : sourcePager)
const activeItems = computed(() => props.mode === 'visits' ? visits.value : sources.value)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  const pager = activePager.value
  if (pager.loading.value && !activeItems.value.length) return 'loading'
  if (pager.errorStatus.value === 403) return 'forbidden'
  if (pager.error.value) return 'error'
  return activeItems.value.length ? 'ready' : 'empty'
})

async function load(): Promise<void> {
  await activePager.value.refresh()
}

async function loadFirstPage(): Promise<void> {
  await activePager.value.first()
}

onMounted(loadFirstPage)
watch(() => props.mode, () => void loadFirstPage())
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONSENTED FIRST-PARTY ANALYTICS" :title="mode === 'visits' ? '访问趋势' : '站外来源'" :description="mode === 'visits' ? '只显示按日期、Landing Path 聚合的访问与转化，不提供单访客记录或画像。' : '按日期、来源域与清洗后的 UTM 展示聚合，不建立个人画像。'">
      <template #actions><RouterLink class="button button--secondary" to="/analytics">返回总览</RouterLink><button class="button button--primary" type="button" @click="load"><RefreshCw :size="16" />刷新</button></template>
    </PageHeader>
    <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>最小化归因数据</strong><p>数据库不保存 IP、User-Agent、完整 Referrer、查询参数、表单正文或 PII。</p></div><StatusBadge label="Consent required" tone="success" /></div>
    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? (mode === 'visits' ? '暂无已同意访问' : '暂无来源聚合') : ''" @retry="load" />
    <section v-else-if="mode === 'visits'" class="panel table-panel"><div class="data-table-wrap"><table class="data-table"><thead><tr><th>日期</th><th>Landing Path</th><th>Locale</th><th>访问 / 浏览</th><th>RFQ 开始 / 提交</th></tr></thead><tbody><tr v-for="visit in visits" :key="`${visit.bucketDate}-${visit.locale}-${visit.landingPath}`"><td>{{ new Date(`${visit.bucketDate}T00:00:00`).toLocaleDateString('zh-CN') }}</td><td><code>{{ visit.landingPath }}</code></td><td>{{ visit.locale }}</td><td>{{ visit.visits }} / {{ visit.pageViews }}</td><td>{{ visit.rfqStarts }} / {{ visit.rfqSubmissions }}</td></tr></tbody></table></div><CursorPaginationControls :item-count="visits.length" :page-number="visitPager.pageNumber.value" :can-previous="visitPager.canPrevious.value" :can-next="visitPager.canNext.value" :loading="visitPager.loading.value" label="条访问聚合" @previous="visitPager.previous" @next="visitPager.next" /></section>
    <section v-else class="source-dashboard"><article class="panel source-chart"><header class="panel__header"><div><p class="eyebrow">DAILY ATTRIBUTION</p><h2>来源分布</h2></div><BarChart3 :size="20" /></header><div class="source-bars"><div v-for="item in sources" :key="`${item.bucketDate}-${item.source}-${item.medium}-${item.campaign}-${item.landingPath}`"><span><strong>{{ item.sourceName || item.source }}</strong><small>{{ item.bucketDate }} · {{ item.referrerDomain || 'Direct' }} · {{ item.utmSource || 'no UTM' }} / {{ item.medium || 'none' }}</small></span><i><b :style="{ width: `${Math.max(2, item.visits / maxVisits * 100)}%` }"></b></i><em>{{ item.visits }}</em></div></div><CursorPaginationControls :item-count="sources.length" :page-number="sourcePager.pageNumber.value" :can-previous="sourcePager.canPrevious.value" :can-next="sourcePager.canNext.value" :loading="sourcePager.loading.value" label="条来源聚合" @previous="sourcePager.previous" @next="sourcePager.next" /></article><article class="panel source-guidance"><ExternalLink :size="22" /><h2>来源分类规则</h2><p>服务端仅保留规范化域名、无查询参数 Landing Path 与清洗后的 UTM；本页读取每日聚合，不返回匿名会话 ID。</p><ul><li>Raw visit/event：180 天</li><li>每日来源聚合：24 个月</li><li>无 Analytics consent：不创建记录</li></ul></article></section>
  </div>
</template>
