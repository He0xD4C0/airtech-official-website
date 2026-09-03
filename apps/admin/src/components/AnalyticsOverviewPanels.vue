<script setup lang="ts">
import { computed } from 'vue'
import { Activity, Eye, Inbox, MessageSquareText, MousePointerClick, Send, TriangleAlert, UsersRound } from 'lucide-vue-next'
import type { AnalyticsOverview } from '@/services/adminApi'
import MetricCard from '@/components/MetricCard.vue'
import StatusBadge from '@/components/StatusBadge.vue'

const props = defineProps<{ overview: AnalyticsOverview }>()

const hasConsentedData = computed(() => {
  const metrics = props.overview.consentedMetrics
  return metrics.visits > 0
    || metrics.pageViews > 0
    || metrics.engagedVisitDays > 0
    || metrics.rfqStartEvents > 0
    || metrics.rfqSubmitEvents > 0
})
</script>

<template>
  <div class="operations-warning" role="note">
    <TriangleAlert :size="18" />
    <div><strong>当前计数限制</strong><p>事件写入去重仍待下一增量完成，行为事件数可能受重试影响。</p></div>
  </div>

  <section class="metric-grid analytics-metric-grid" aria-label="已同意 Analytics 指标">
    <MetricCard label="已同意访问" :value="String(overview.consentedMetrics.visits)" detail="首次出现于所选 UTC 日期范围" :icon="UsersRound" tone="blue" />
    <MetricCard label="页面浏览事件" :value="String(overview.consentedMetrics.pageViews)" detail="已接受 pageView" :icon="Eye" tone="blue" />
    <MetricCard label="参与访问日" :value="String(overview.consentedMetrics.engagedVisitDays)" detail="达到参与条件的访问日" :icon="Activity" tone="green" />
    <MetricCard label="RFQ 开始事件" :value="String(overview.consentedMetrics.rfqStartEvents)" detail="已接受 rfqStarted" :icon="MousePointerClick" tone="amber" />
    <MetricCard label="可归因 RFQ 提交事件" :value="String(overview.consentedMetrics.rfqSubmitEvents)" detail="已接受 rfqSubmitted" :icon="Send" tone="green" />
  </section>

  <p v-if="!hasConsentedData" class="analytics-empty-note">所选范围暂无已同意行为数据。</p>

  <section class="panel analytics-business-outcomes" aria-labelledby="business-outcomes-title">
    <header class="panel__header">
      <div><p class="eyebrow">BUSINESS OUTCOMES</p><h2 id="business-outcomes-title">独立业务结果</h2></div>
      <StatusBadge label="不参与行为漏斗" tone="info" />
    </header>
    <dl class="analytics-business-outcomes__grid">
      <div><dt><Inbox :size="15" />RFQ 提交记录</dt><dd>{{ overview.businessOutcomes.rfqSubmissions }}</dd></div>
      <div><dt><MessageSquareText :size="15" />Contact 提交记录</dt><dd>{{ overview.businessOutcomes.contactRequests }}</dd></div>
    </dl>
    <p class="analytics-business-outcomes__note">业务记录与已同意行为事件属于不同统计集合；本页不会将二者相除或拼成转化漏斗。</p>
  </section>
</template>
