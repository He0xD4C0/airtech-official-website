<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { AdminDashboardSummary, DashboardMetric } from '@airtek/contracts'
import { ArrowRight, BookOpenText, Boxes, Cable, CheckCircle2, CircleAlert, Eye, Inbox, ListTodo, MousePointerClick, TrendingUp } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import MetricCard from '@/components/MetricCard.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const auth = useAuthStore()
const ui = useUiStore()
const summary = ref<AdminDashboardSummary | null>(null)
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const errorMessage = ref('')

function metricValue(metric?: DashboardMetric): string {
  return metric?.available ? String(metric.value ?? 0) : '—'
}

const tasks = computed(() => {
  const value = summary.value
  if (!value) return []
  return [
    value.openConflicts.available ? { title: '检查 Feishu 字段冲突', detail: `${value.openConflicts.value ?? 0} 个冲突等待处理`, icon: Cable, tone: 'danger', to: '/integrations/feishu' } : null,
    value.draftContent.available ? { title: '处理私人草稿', detail: `${value.draftContent.value ?? 0} 个草稿记录`, icon: BookOpenText, tone: 'warning', to: '/content/drafts' } : null,
    value.openRfqs.available ? { title: '查看 RFQ 收件箱', detail: `${value.openRfqs.value ?? 0} 条未关闭 RFQ`, icon: Inbox, tone: 'blue', to: '/rfqs' } : null,
  ].filter((task): task is NonNullable<typeof task> => task !== null)
})

async function loadDashboard(): Promise<void> {
  state.value = 'loading'
  errorMessage.value = ''
  try {
    summary.value = await adminApi.dashboardSummary()
    state.value = 'ready'
  } catch (error) {
    summary.value = null
    errorMessage.value = apiErrorMessage(error, '请检查 API 会话。')
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    ui.toast('工作台数据读取失败', errorMessage.value, 'danger')
  }
}

onMounted(loadDashboard)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="OVERVIEW" title="早上好，开始今天的发布工作" description="权限感知的全局计数、分析信号与工作项全部由服务端汇总。">
      <template #actions>
        <RouterLink v-if="auth.hasPermission('content.read')" class="button button--secondary" to="/content/published"><Eye :size="16" />查看已发布内容</RouterLink>
        <RouterLink v-if="auth.hasPermission('content.write')" class="button button--primary" to="/content/drafts/new"><BookOpenText :size="16" />新建私人草稿</RouterLink>
      </template>
    </PageHeader>

    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'error' ? errorMessage : ''" description="服务端汇总可用后会显示在这里。" @retry="loadDashboard" />

    <template v-else-if="summary">
      <section class="metric-grid" aria-label="关键指标">
        <MetricCard label="待发布内容" :value="metricValue(summary.draftContent)" :detail="summary.draftContent.unavailableReason || '全部草稿记录'" :icon="BookOpenText" tone="blue" />
        <MetricCard label="产品数据冲突" :value="metricValue(summary.openConflicts)" :detail="summary.openConflicts.unavailableReason || '全部未解决冲突'" :icon="Boxes" tone="amber" />
        <MetricCard label="开放 RFQ" :value="metricValue(summary.openRfqs)" :detail="summary.openRfqs.unavailableReason || '不含 closed 与 spam'" :icon="Inbox" tone="green" />
      </section>

      <section class="dashboard-grid">
        <article class="panel performance-panel">
          <header class="panel__header"><div><p class="eyebrow">SITE SIGNAL</p><h2>公开站关键行为</h2></div><span class="inline-note">最近 30 个 UTC 日历日</span></header>
          <div v-if="summary.analytics" class="performance-summary"><div><span>已同意访问</span><strong>{{ summary.analytics.visits }}</strong><em><TrendingUp :size="13" />第一方聚合</em></div><div><span>页面浏览事件</span><strong>{{ summary.analytics.pageViews }}</strong><small>已接受 pageView</small></div><div><span>可归因 RFQ 提交事件</span><strong>{{ summary.analytics.rfqSubmitEvents }}</strong><small>与业务 RFQ 记录独立</small></div></div>
          <p v-else class="empty-mini">当前角色没有 analytics.read 权限。</p>
          <footer class="panel__footer"><RouterLink v-if="summary.analytics" to="/analytics">打开 Analytics <ArrowRight :size="14" /></RouterLink></footer>
        </article>

        <article class="panel task-panel">
          <header class="panel__header"><div><p class="eyebrow">PRIORITY</p><h2>需要处理</h2></div><ListTodo :size="20" /></header>
          <RouterLink v-for="task in tasks" :key="task.title" :to="task.to" class="task-row"><span class="task-row__icon" :class="`task-row__icon--${task.tone}`"><component :is="task.icon" :size="18" /></span><span><strong>{{ task.title }}</strong><small>{{ task.detail }}</small></span><ArrowRight :size="15" /></RouterLink>
          <footer class="panel__footer panel__footer--between"><span>全局计数来自服务端</span><button type="button" @click="loadDashboard">刷新</button></footer>
        </article>
      </section>

      <section class="dashboard-grid dashboard-grid--bottom">
        <article class="panel activity-panel">
          <header class="panel__header"><div><p class="eyebrow">AUDIT TRAIL</p><h2>最近活动</h2></div><RouterLink v-if="auth.hasPermission('audit.read')" to="/audit">全部日志</RouterLink></header>
          <div class="activity-list"><div v-for="activity in summary.recentActivity" :key="activity.id" class="activity-row"><span class="activity-row__icon activity-row__icon--success"><CheckCircle2 :size="17" /></span><div><strong>{{ activity.action }}</strong><p>{{ activity.actor }} · {{ new Date(activity.occurredAt).toLocaleString('zh-CN') }}</p></div></div><p v-if="!summary.recentActivity.length" class="empty-mini">没有可见的最近审计活动。</p></div>
        </article>

        <article class="panel readiness-panel">
          <header class="panel__header"><div><p class="eyebrow">PUBLISHING READINESS</p><h2>发布工作项</h2></div><StatusBadge :label="summary.readinessItemCount ? '需要处理' : '无已知工作项'" :tone="summary.readinessItemCount ? 'warning' : 'success'" /></header>
          <div class="readiness-score"><strong>{{ summary.readinessItemCount }}</strong><span>项待处理</span></div>
          <ul class="readiness-list"><li><CircleAlert :size="16" />{{ metricValue(summary.openConflicts) }} 个来源冲突</li><li><MousePointerClick :size="16" />{{ metricValue(summary.draftContent) }} 个内容草稿</li></ul>
        </article>
      </section>
    </template>
  </div>
</template>
