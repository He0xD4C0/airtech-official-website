<script setup lang="ts">
import { analyticsApi } from '@/features/analytics/services/analyticsApi'
import { computed, ref, watch } from 'vue'
import { Route, ShieldCheck } from 'lucide-vue-next'
import { useRoute, useRouter } from 'vue-router'
import AnalyticsOverviewPanels from '@/features/analytics/components/AnalyticsOverviewPanels.vue'
import AnalyticsRangePicker from '@/features/analytics/components/AnalyticsRangePicker.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import StatusBadge from '@/shared/components/StatusBadge.vue'
import { type AnalyticsOverview } from '@/shared/services/adminApiTypes'
import { activeAnalyticsPreset, analyticsApiRange, analyticsPresetAction, analyticsRangeForPreset, resolveAnalyticsDateRange, type AnalyticsDateRange, type AnalyticsPresetDays } from '@/features/analytics/services/analyticsDateRange'
import { apiErrorMessage, apiProblemStatus } from '@/shared/services/cursorPagination'
import { useUiStore } from '@/shared/stores/ui'

const route = useRoute()
const router = useRouter()
const ui = useUiStore()
const presetReference = ref(new Date())
const range = ref<AnalyticsDateRange>(analyticsRangeForPreset(30, presetReference.value))
const overview = ref<AnalyticsOverview | null>(null)
const state = ref<'loading' | 'ready' | 'error' | 'forbidden'>('loading')
const errorMessage = ref('')
let requestSequence = 0

const activePreset = computed(() => activeAnalyticsPreset(range.value, presetReference.value))
const generatedAt = computed(() => overview.value
  ? new Intl.DateTimeFormat('zh-CN', {
      timeZone: 'UTC',
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(new Date(overview.value.generatedAt))
  : '')

async function load(nextRange = range.value): Promise<void> {
  const requestId = ++requestSequence
  state.value = 'loading'
  errorMessage.value = ''
  try {
    const value = await analyticsApi.analyticsOverview(analyticsApiRange(nextRange))
    if (requestId !== requestSequence) return
    overview.value = value
    state.value = 'ready'
  } catch (error) {
    if (requestId !== requestSequence) return
    errorMessage.value = apiErrorMessage(error, '请检查 API 会话或 Analytics 日期范围。')
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    ui.toast('Analytics 读取失败', errorMessage.value, 'danger')
  }
}

async function syncRangeFromUrl(): Promise<void> {
  const resolved = resolveAnalyticsDateRange(route.query.from, route.query.to, presetReference.value)
  range.value = resolved.range
  if (!resolved.canonical) {
    await router.replace({
      query: { ...route.query, from: resolved.range.from, to: resolved.range.to },
    })
    return
  }
  await load(resolved.range)
}

async function selectPreset(days: AnalyticsPresetDays): Promise<void> {
  presetReference.value = new Date()
  const action = analyticsPresetAction(days, range.value, presetReference.value)
  if (action.reload) {
    await load(action.range)
    return
  }
  await router.replace({ query: { ...route.query, from: action.range.from, to: action.range.to } })
}

watch(
  () => [route.query.from, route.query.to],
  () => void syncRangeFromUrl(),
  { immediate: true },
)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FIRST-PARTY ANALYTICS" title="网站表现" description="在统一 UTC 时间窗内查看已同意的第一方行为；RFQ 与 Contact 业务记录独立呈现，不伪装成行为漏斗。">
      <template #actions><RouterLink class="button button--primary" to="/analytics/sources"><Route :size="16" />来源分析</RouterLink></template>
    </PageHeader>

    <div class="analytics-toolbar"><div><span class="is-active" aria-current="page">总览</span><RouterLink to="/analytics/sources">站外来源</RouterLink></div><span class="inline-note">第一方聚合 · UTC</span></div>

    <AnalyticsRangePicker
      :range="range"
      :active-preset="activePreset"
      :loading="state === 'loading'"
      @select="selectPreset"
    />

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'error' ? errorMessage : ''"
      @retry="load()"
    />

    <template v-else-if="overview">
      <div class="security-baseline">
        <ShieldCheck :size="18" />
        <div><strong>Consent-aware 聚合</strong><p>来源 {{ overview.source }} · 生成于 {{ generatedAt }} UTC；响应契约不包含 PII。</p></div>
        <StatusBadge label="白名单属性" tone="success" />
      </div>
      <AnalyticsOverviewPanels :overview="overview" />
    </template>
  </div>
</template>
