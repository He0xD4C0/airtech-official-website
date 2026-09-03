<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  ArrowRight,
  BookOpenText,
  Boxes,
  Cable,
  CheckCircle2,
  CircleAlert,
  Eye,
  Inbox,
  ListTodo,
  MousePointerClick,
  RefreshCw,
  TrendingUp,
} from 'lucide-vue-next'
import MetricCard from '@/components/MetricCard.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi } from '@/services/adminApi'
import { analyticsApiRange, analyticsRangeForPreset } from '@/services/analyticsDateRange'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const auth = useAuthStore()
const ui = useUiStore()

const metrics = ref<{
  drafts: number | null
  conflicts: number | null
  rfqs: number | null
  operations: number | null
  visits: number | null
  pageViews: number | null
  rfqSubmitEvents: number | null
}>({
  drafts: null,
  conflicts: null,
  rfqs: null,
  operations: null,
  visits: null,
  pageViews: null,
  rfqSubmitEvents: null,
})
const hasMore = ref({ drafts: false, conflicts: false, rfqs: false, operations: false })
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>('loading')
const errorMessage = ref('')
const activities = ref<Array<{ action: string; actor: string; when: string }>>([])
const metricValue = (key: 'drafts' | 'conflicts' | 'rfqs' | 'operations') => {
  const value = metrics.value[key]
  return value === null ? '—' : `${value}${hasMore.value[key] ? '+' : ''}`
}

const tasks = computed(() => [
  auth.hasPermission('integration.run') ? { title: '检查 Feishu 字段冲突', detail: `${metricValue('conflicts')} 个冲突等待处理`, icon: Cable, tone: 'danger', to: '/integrations/feishu' } : null,
  auth.hasPermission('content.read') ? { title: '处理内容工作草稿', detail: `${metricValue('drafts')} 个草稿、计划或占位记录`, icon: BookOpenText, tone: 'warning', to: '/content' } : null,
  auth.hasPermission('rfq.read') ? { title: '查看 RFQ 收件箱', detail: `${metricValue('rfqs')} 条 RFQ 记录`, icon: Inbox, tone: 'blue', to: '/rfqs' } : null,
].filter((task): task is NonNullable<typeof task> => task !== null))

async function loadDashboard(): Promise<void> {
  state.value = 'loading'
  errorMessage.value = ''
  metrics.value = {
    drafts: null,
    conflicts: null,
    rfqs: null,
    operations: null,
    visits: null,
    pageViews: null,
    rfqSubmitEvents: null,
  }
  hasMore.value = { drafts: false, conflicts: false, rfqs: false, operations: false }
  activities.value = []
  try {
    const requests: Array<Promise<void>> = []
    if (auth.hasPermission('content.read')) requests.push(adminApi.listContent({ limit: 100 }).then((content) => {
      metrics.value.drafts = content.items.filter((entry) => entry.status !== 'published').length
      hasMore.value.drafts = Boolean(content.nextCursor)
    }))
    if (auth.hasPermission('integration.run')) requests.push(adminApi.listConflicts({ limit: 100 }).then((conflicts) => {
      metrics.value.conflicts = conflicts.items.length
      hasMore.value.conflicts = Boolean(conflicts.nextCursor)
    }))
    if (auth.hasPermission('operations.run')) requests.push(adminApi.listOperations({ limit: 100 }).then((operations) => {
      metrics.value.operations = operations.items.filter((operation) => ['queued', 'running'].includes(operation.status)).length
      hasMore.value.operations = Boolean(operations.nextCursor)
    }))
    if (auth.hasPermission('analytics.read')) requests.push(adminApi.analyticsOverview(
      analyticsApiRange(analyticsRangeForPreset(30)),
    ).then((analytics) => {
      metrics.value.visits = analytics.consentedMetrics.visits
      metrics.value.pageViews = analytics.consentedMetrics.pageViews
      metrics.value.rfqSubmitEvents = analytics.consentedMetrics.rfqSubmitEvents
    }))
    if (auth.hasPermission('rfq.read')) requests.push(adminApi.listRfqs({ limit: 100 }).then((rfqs) => {
      metrics.value.rfqs = rfqs.items.length
      hasMore.value.rfqs = Boolean(rfqs.nextCursor)
    }))
    if (auth.hasPermission('audit.read')) requests.push(adminApi.listAudit({ limit: 10 }).then((audit) => {
      activities.value = audit.items.map((event) => ({
        action: event.action,
        actor: event.actor,
        when: new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(event.occurredAt)),
      }))
    }))
    await Promise.all(requests)
    const knownValues = Object.values(metrics.value).filter((value): value is number => value !== null)
    state.value = knownValues.some(Boolean) || activities.value.length ? 'ready' : 'empty'
  } catch (error) {
    errorMessage.value = apiErrorMessage(error, '请检查 API 会话。')
    state.value = apiProblemStatus(error) === 403 ? 'forbidden' : 'error'
    ui.toast('工作台数据读取失败', errorMessage.value, 'danger')
  }
}

onMounted(loadDashboard)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="OVERVIEW" title="早上好，开始今天的发布工作" description="从数据来源到公开页面，集中查看需要处理的内容、产品与询盘。">
      <template #actions>
        <RouterLink class="button button--secondary" to="/content"><Eye :size="16" />查看内容</RouterLink>
        <RouterLink class="button button--primary" to="/content/new/edit"><BookOpenText :size="16" />新建内容</RouterLink>
      </template>
    </PageHeader>

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? '数据库中暂无工作台活动' : state === 'error' ? errorMessage : ''"
      description="内容、同步、询盘、任务或审计记录产生后会显示在这里。"
      @retry="loadDashboard"
    />

    <template v-else>
    <section class="metric-grid" aria-label="关键指标">
      <MetricCard label="待发布内容" :value="metricValue('drafts')" :detail="metrics.drafts === null ? '当前角色不可读取内容' : hasMore.drafts ? '前 100 条中的草稿、计划或占位记录' : '草稿、计划或占位记录'" :icon="BookOpenText" tone="blue" />
      <MetricCard label="产品数据冲突" :value="metricValue('conflicts')" :detail="metrics.conflicts === null ? '当前角色不可读取同步冲突' : hasMore.conflicts ? '前 100 条；阻止下一次产品发布' : '阻止下一次产品发布'" :icon="Boxes" tone="amber" />
      <MetricCard label="RFQ 记录" :value="metricValue('rfqs')" :detail="metrics.rfqs === null ? '当前角色不可读取 RFQ' : '按 PII 权限隔离'" :icon="Inbox" tone="green" />
      <MetricCard label="运行中任务" :value="metricValue('operations')" :detail="hasMore.operations ? '前 100 条中排队或执行中的任务' : '排队或执行中'" :icon="RefreshCw" tone="slate" />
    </section>

    <section class="dashboard-grid">
      <article class="panel performance-panel">
        <header class="panel__header">
          <div><p class="eyebrow">SITE SIGNAL</p><h2>公开站关键行为</h2></div>
          <span class="inline-note">最近 30 个 UTC 日历日</span>
        </header>
        <div class="performance-summary">
          <div><span>已同意访问</span><strong>{{ metrics.visits ?? '—' }}</strong><em><TrendingUp :size="13" />第一方聚合</em></div>
          <div><span>页面浏览事件</span><strong>{{ metrics.pageViews ?? '—' }}</strong><small>已接受 pageView</small></div>
          <div><span>可归因 RFQ 提交事件</span><strong>{{ metrics.rfqSubmitEvents ?? '—' }}</strong><small>与业务 RFQ 记录独立</small></div>
        </div>
        <p class="empty-mini">这里只显示已同意行为的 30 天汇总；业务 RFQ 数量由 RFQ API 单独读取。</p>
        <footer class="panel__footer"><span><i class="legend-dot legend-dot--blue"></i>内容互动</span><span><i class="legend-dot legend-dot--green"></i>高意向操作</span><RouterLink to="/analytics">打开 Analytics <ArrowRight :size="14" /></RouterLink></footer>
      </article>

      <article class="panel task-panel">
        <header class="panel__header"><div><p class="eyebrow">PRIORITY</p><h2>需要处理</h2></div><ListTodo :size="20" /></header>
        <RouterLink v-for="task in tasks" :key="task.title" :to="task.to" class="task-row">
          <span class="task-row__icon" :class="`task-row__icon--${task.tone}`"><component :is="task.icon" :size="18" /></span>
          <span><strong>{{ task.title }}</strong><small>{{ task.detail }}</small></span>
          <ArrowRight :size="15" />
        </RouterLink>
        <footer class="panel__footer panel__footer--between"><span>数量来自当前 API 页</span><button type="button" @click="loadDashboard">刷新</button></footer>
      </article>
    </section>

    <section class="dashboard-grid dashboard-grid--bottom">
      <article class="panel activity-panel">
        <header class="panel__header"><div><p class="eyebrow">AUDIT TRAIL</p><h2>最近活动</h2></div><RouterLink to="/audit">全部日志</RouterLink></header>
        <div class="activity-list">
          <div v-for="activity in activities" :key="`${activity.action}-${activity.when}`" class="activity-row">
            <span class="activity-row__icon activity-row__icon--success"><CheckCircle2 :size="17" /></span>
            <div><strong>{{ activity.action }}</strong><p>{{ activity.actor }} · {{ activity.when }}</p></div>
          </div>
          <p v-if="!activities.length" class="empty-mini">审计 API 暂无最近活动。</p>
        </div>
      </article>

      <article class="panel readiness-panel">
        <header class="panel__header"><div><p class="eyebrow">PUBLISHING READINESS</p><h2>发布准备度</h2></div><StatusBadge :label="(metrics.conflicts ?? 0) + (metrics.drafts ?? 0) ? '需要处理' : '当前页无阻塞'" :tone="(metrics.conflicts ?? 0) + (metrics.drafts ?? 0) ? 'warning' : 'success'" /></header>
        <div class="readiness-score"><strong>{{ (metrics.conflicts ?? 0) + (metrics.drafts ?? 0) }}</strong><span>项待处理</span></div>
        <ul class="readiness-list">
          <li><CheckCircle2 :size="16" />路由与 SEO 基础已就绪</li>
          <li><CircleAlert :size="16" />{{ metricValue('conflicts') }} 个产品来源冲突</li>
          <li><MousePointerClick :size="16" />{{ metricValue('drafts') }} 个内容记录待处理</li>
        </ul>
        <RouterLink class="button button--secondary button--wide" to="/operations">查看发布检查</RouterLink>
      </article>
    </section>
    </template>
  </div>
</template>
