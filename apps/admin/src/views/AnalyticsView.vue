<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CheckCircle2, Cookie, Eye, MousePointerClick, Route, ShieldCheck, Target } from 'lucide-vue-next'
import MetricCard from '@/components/MetricCard.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi } from '@/services/adminApi'
import { apiProblemStatus } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const summary = ref({ acceptedEventCount: 0, rfqCount: 0, contactCount: 0 })
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const funnel = computed(() => [
  { label: '已接受匿名事件', value: String(summary.value.acceptedEventCount), width: summary.value.acceptedEventCount ? 100 : 0 },
  { label: 'RFQ 提交', value: String(summary.value.rfqCount), width: summary.value.acceptedEventCount ? Math.min(100, summary.value.rfqCount / summary.value.acceptedEventCount * 100) : 0 },
  { label: 'Contact 提交', value: String(summary.value.contactCount), width: summary.value.acceptedEventCount ? Math.min(100, summary.value.contactCount / summary.value.acceptedEventCount * 100) : 0 },
])

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    const value = await adminApi.analyticsSummary()
    summary.value = value
    state.value = value.acceptedEventCount || value.rfqCount || value.contactCount ? 'ready' : 'empty'
  } catch (error) {
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    ui.toast('Analytics 读取失败', error instanceof Error ? error.message : '请检查 API 会话。', 'danger')
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FIRST-PARTY ANALYTICS" title="网站表现" description="围绕内容发现、产品互动与 RFQ 转化衡量质量；自由文本和个人信息永不进入事件属性。">
      <template #actions><RouterLink class="button button--secondary" to="/analytics/visits"><Eye :size="16" />访问趋势</RouterLink><RouterLink class="button button--primary" to="/analytics/sources"><Route :size="16" />来源分析</RouterLink></template>
    </PageHeader>

    <div class="analytics-toolbar"><div><button class="is-active" type="button">总览</button><RouterLink to="/analytics/visits">访问趋势</RouterLink><RouterLink to="/analytics/sources">站外来源</RouterLink><button type="button">RFQ 漏斗</button><button type="button">事件字典</button></div><span class="inline-note">全部可用聚合 · API 暂未提供时间范围筛选</span></div>

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? '暂无已同意 Analytics 数据' : ''"
      @retry="load"
    />

    <section v-if="state === 'ready'" class="metric-grid">
      <MetricCard label="已接受匿名事件" :value="String(summary.acceptedEventCount)" detail="第一方白名单事件" :icon="Eye" tone="blue" />
      <MetricCard label="Contact 提交" :value="String(summary.contactCount)" detail="按 PII 权限隔离" :icon="MousePointerClick" tone="green" />
      <MetricCard label="RFQ 提交" :value="String(summary.rfqCount)" detail="四类结构化入口" :icon="Route" tone="amber" />
      <MetricCard label="PII 进入事件" value="0" detail="API 固定返回 containsPii=false" :icon="Target" tone="slate" />
    </section>

    <section v-if="state === 'ready'" class="analytics-grid">
      <article class="panel funnel-panel">
        <header class="panel__header"><div><p class="eyebrow">QUALIFIED JOURNEY</p><h2>转化路径</h2></div><StatusBadge label="第一方 API" tone="success" /></header>
        <div class="funnel-bars"><div v-for="step in funnel" :key="step.label"><span><strong>{{ step.label }}</strong><em>{{ step.value }}</em></span><i :style="{ width: `${step.width}%` }"></i></div></div>
        <p class="chart-note">该漏斗只使用允许的阶段事件，不读取 RFQ 表单正文。</p>
      </article>

      <article class="panel consent-panel">
        <header class="panel__header"><div><p class="eyebrow">PRIVACY</p><h2>Consent 健康状态</h2></div><Cookie :size="20" /></header>
        <div class="consent-score"><strong>严格 opt-in</strong><StatusBadge label="规则有效" tone="success" /></div>
        <ul><li><CheckCircle2 :size="16" />必要安全日志不依赖行为同意</li><li><CheckCircle2 :size="16" />第一方行为事件等待同意</li><li><CheckCircle2 :size="16" />GA4 未同意时不加载</li><li><ShieldCheck :size="16" />事件属性执行 PII 白名单</li></ul>
        <RouterLink class="button button--secondary button--wide" to="/settings/consent">管理 Consent</RouterLink>
      </article>
    </section>

    <DataStatePanel v-if="state === 'ready'" state="empty" title="事件字典端点尚未提供" description="后台不会以硬编码事件列表替代服务端实际白名单。" />
  </div>
</template>
