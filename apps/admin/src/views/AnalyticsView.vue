<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { BarChart3, CheckCircle2, ChevronDown, Cookie, Download, Eye, Filter, MousePointerClick, Route, ShieldCheck, Target } from 'lucide-vue-next'
import MetricCard from '@/components/MetricCard.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const summary = ref({ acceptedEventCount: mockApiEnabled ? 864 : 0, rfqCount: mockApiEnabled ? 11 : 0, contactCount: mockApiEnabled ? 4 : 0 })
const funnel = computed(() => mockApiEnabled ? [
  { label: '公开页面浏览', value: '1,284', width: 100 },
  { label: '产品 / 内容互动', value: '386', width: 72 },
  { label: 'RFQ 开始', value: '42', width: 48 },
  { label: 'RFQ 提交', value: '11', width: 28 },
] : [
  { label: '已接受匿名事件', value: String(summary.value.acceptedEventCount), width: summary.value.acceptedEventCount ? 100 : 0 },
  { label: 'RFQ 提交', value: String(summary.value.rfqCount), width: summary.value.acceptedEventCount ? Math.min(100, summary.value.rfqCount / summary.value.acceptedEventCount * 100) : 0 },
  { label: 'Contact 提交', value: String(summary.value.contactCount), width: summary.value.acceptedEventCount ? Math.min(100, summary.value.contactCount / summary.value.acceptedEventCount * 100) : 0 },
])

const eventRows = [
  ['pageView', 'Public Web', 'sourcePath, locale, contentType', 'Consent required'],
  ['filterApplied', 'Products', 'filterGroup, selectedCount', 'Consent required'],
  ['selectorResult', 'Selector', 'outcome, candidateCount', 'Consent required'],
  ['rfqStepCompleted', 'RFQ', 'journey, step, outcome', 'Consent required'],
  ['ctaClicked', 'Public Web', 'ctaId, sourcePath', 'Consent required'],
]

onMounted(async () => {
  if (mockApiEnabled) return
  try {
    const value = await adminApi.analyticsSummary()
    summary.value = value
  } catch (error) {
    ui.toast('Analytics 读取失败', error instanceof Error ? error.message : '请检查 API 会话。', 'danger')
  }
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FIRST-PARTY ANALYTICS" title="网站表现" description="围绕内容发现、产品互动与 RFQ 转化衡量质量；自由文本和个人信息永不进入事件属性。">
      <template #actions><button class="button button--secondary" type="button"><Download :size="16" />导出聚合数据</button><button class="button button--primary" type="button"><Filter :size="16" />创建视图</button></template>
    </PageHeader>

    <div class="analytics-toolbar"><div><button class="is-active" type="button">总览</button><button type="button">内容</button><button type="button">产品</button><button type="button">RFQ 漏斗</button><button type="button">事件字典</button></div><button type="button">最近 30 天<ChevronDown :size="14" /></button></div>

    <section class="metric-grid">
      <MetricCard label="已接受匿名事件" :value="String(summary.acceptedEventCount)" detail="第一方白名单事件" :icon="Eye" tone="blue" />
      <MetricCard label="Contact 提交" :value="String(summary.contactCount)" detail="按 PII 权限隔离" :icon="MousePointerClick" tone="green" />
      <MetricCard label="RFQ 提交" :value="String(summary.rfqCount)" detail="四类结构化入口" :icon="Route" tone="amber" />
      <MetricCard label="PII 进入事件" value="0" detail="API 固定返回 containsPii=false" :icon="Target" tone="slate" />
    </section>

    <section class="analytics-grid">
      <article class="panel funnel-panel">
        <header class="panel__header"><div><p class="eyebrow">QUALIFIED JOURNEY</p><h2>转化路径</h2></div><StatusBadge :label="mockApiEnabled ? '演示数据' : '第一方 API'" :tone="mockApiEnabled ? 'neutral' : 'success'" /></header>
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

    <section class="panel table-panel event-dictionary">
      <header class="panel__header"><div><p class="eyebrow">EVENT DICTIONARY</p><h2>事件白名单</h2></div><button class="button button--quiet" type="button"><BarChart3 :size="16" />管理事件</button></header>
      <div class="data-table-wrap"><table class="data-table"><thead><tr><th>事件</th><th>来源</th><th>允许属性</th><th>Consent 类别</th></tr></thead><tbody><tr v-for="row in eventRows" :key="row[0]"><td><code>{{ row[0] }}</code></td><td>{{ row[1] }}</td><td>{{ row[2] }}</td><td><StatusBadge :label="row[3]" :tone="row[3] === 'Necessary' ? 'success' : 'info'" /></td></tr></tbody></table></div>
    </section>
  </div>
</template>
