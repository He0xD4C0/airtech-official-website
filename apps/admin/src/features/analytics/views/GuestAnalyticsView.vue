<script setup lang="ts">
import { analyticsApi } from '@/features/analytics/services/analyticsApi'
import { computed, onMounted } from 'vue'
import { BarChart3, ExternalLink, RefreshCw, ShieldCheck } from 'lucide-vue-next'
import CursorPaginationControls from '@/shared/components/CursorPaginationControls.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import StatusBadge from '@/shared/components/StatusBadge.vue'
import { useCursorPagination } from '@/shared/composables/useCursorPagination'
import { type GuestSourceDaily } from '@/shared/services/adminApiTypes'

const sourcePager = useCursorPagination<GuestSourceDaily>(
  (pagination) => analyticsApi.listGuestSources(pagination),
  { errorMessage: '来源聚合读取失败。' },
)
const sources = computed(() => sourcePager.items.value)
const maxVisits = computed(() => Math.max(1, ...sources.value.map((item) => item.visits)))
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (sourcePager.loading.value && !sources.value.length) return 'loading'
  if (sourcePager.errorStatus.value === 403) return 'forbidden'
  if (sourcePager.error.value) return 'error'
  return sources.value.length ? 'ready' : 'empty'
})

onMounted(() => sourcePager.first())
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONSENTED FIRST-PARTY ANALYTICS" title="站外来源" description="按日期、来源域与清洗后的 UTM 展示聚合，不建立个人画像。">
      <template #actions><RouterLink class="button button--secondary" to="/analytics">返回总览</RouterLink><button class="button button--primary" type="button" @click="sourcePager.refresh"><RefreshCw :size="16" />刷新</button></template>
    </PageHeader>
    <div class="security-baseline"><ShieldCheck :size="18" /><div><strong>最小化归因数据</strong><p>数据库不保存 IP、User-Agent、完整 Referrer、查询参数、表单正文或 PII。</p></div><StatusBadge label="Consent required" tone="success" /></div>
    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '暂无来源聚合' : ''" @retry="sourcePager.refresh" />
    <section v-else class="source-dashboard"><article class="panel source-chart"><header class="panel__header"><div><p class="eyebrow">DAILY ATTRIBUTION</p><h2>来源分布</h2></div><BarChart3 :size="20" /></header><div class="source-bars"><div v-for="item in sources" :key="`${item.bucketDate}-${item.source}-${item.medium}-${item.campaign}-${item.landingPath}`"><span><strong>{{ item.sourceName || item.source }}</strong><small>{{ item.bucketDate }} · {{ item.referrerDomain || 'Direct' }} · {{ item.utmSource || 'no UTM' }} / {{ item.medium || 'none' }}</small></span><i><b :style="{ width: `${Math.max(2, item.visits / maxVisits * 100)}%` }"></b></i><em>{{ item.visits }}</em></div></div><CursorPaginationControls :item-count="sources.length" :page-number="sourcePager.pageNumber.value" :can-previous="sourcePager.canPrevious.value" :can-next="sourcePager.canNext.value" :loading="sourcePager.loading.value" label="条来源聚合" @previous="sourcePager.previous" @next="sourcePager.next" /></article><article class="panel source-guidance"><ExternalLink :size="22" /><h2>来源分类规则</h2><p>服务端仅保留规范化域名、无查询参数 Landing Path 与清洗后的 UTM；本页读取每日聚合，不返回匿名会话 ID。</p><ul><li>Raw visit/event：180 天</li><li>每日来源聚合：24 个月</li><li>无 Analytics consent：不创建记录</li></ul></article></section>
  </div>
</template>
