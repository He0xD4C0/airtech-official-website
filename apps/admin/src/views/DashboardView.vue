<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  ArrowRight,
  BookOpenText,
  Boxes,
  Cable,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Eye,
  FileWarning,
  Inbox,
  ListTodo,
  MousePointerClick,
  RefreshCw,
  TrendingUp,
} from 'lucide-vue-next'
import MetricCard from '@/components/MetricCard.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()

const metrics = ref({ drafts: mockApiEnabled ? 12 : 0, conflicts: mockApiEnabled ? 2 : 0, rfqs: mockApiEnabled ? 4 : 0, operations: mockApiEnabled ? 1 : 0, events: mockApiEnabled ? 1284 : 0 })
const hasMore = ref({ drafts: false, conflicts: false, operations: false })
const metricValue = (key: 'drafts' | 'conflicts' | 'operations') => `${metrics.value[key]}${hasMore.value[key] ? '+' : ''}`

const tasks = computed(() => [
  { title: '检查 Feishu 字段冲突', detail: '2 个阻塞字段等待处理', icon: Cable, tone: 'danger', to: '/integrations/feishu' },
  { title: '完成首页英文内容', detail: '草稿缺少 SEO 描述', icon: BookOpenText, tone: 'warning', to: '/content/home/edit' },
  { title: '分配新 RFQ', detail: '4 条询盘尚未分配', icon: Inbox, tone: 'blue', to: '/rfqs' },
].map((task, index) => mockApiEnabled ? task : ({ ...task, detail: [
  `${hasMore.value.conflicts ? '至少 ' : ''}${metrics.value.conflicts} 个冲突等待处理`,
  `${hasMore.value.drafts ? '至少 ' : ''}${metrics.value.drafts} 个草稿或占位记录`,
  `${metrics.value.rfqs} 条 RFQ 记录`,
][index] })))

const activities = computed(() => mockApiEnabled ? [
  { action: '发布了 Article 草稿', actor: 'Demo Publisher', when: '12 分钟前', icon: CheckCircle2, tone: 'success' },
  { action: '完成 Feishu dry-run', actor: 'Sync Worker', when: '36 分钟前', icon: RefreshCw, tone: 'info' },
  { action: '创建临时字段覆盖', actor: 'Demo Product Manager', when: '2 小时前', icon: FileWarning, tone: 'warning' },
  { action: '运行备份预检', actor: 'Demo Super Admin', when: '昨天 18:42', icon: Clock3, tone: 'neutral' },
] : [])

function acknowledge(): void {
  ui.toast('已记录提醒', '开发演示数据不会触发真实任务。', 'info')
}

onMounted(async () => {
  if (mockApiEnabled) return
  try {
    const [content, conflicts, operations, analytics] = await Promise.all([
      adminApi.listContent({ limit: 100 }),
      adminApi.listConflicts({ limit: 100 }),
      adminApi.listOperations({ limit: 100 }),
      adminApi.analyticsSummary(),
    ])
    metrics.value = {
      drafts: content.items.filter((entry) => entry.status !== 'published').length,
      conflicts: conflicts.items.length,
      rfqs: analytics.rfqCount,
      operations: operations.items.filter((operation) => ['queued', 'running'].includes(operation.status)).length,
      events: analytics.acceptedEventCount,
    }
    hasMore.value = {
      drafts: Boolean(content.nextCursor),
      conflicts: Boolean(conflicts.nextCursor),
      operations: Boolean(operations.nextCursor),
    }
  } catch (error) {
    ui.toast('工作台数据读取失败', error instanceof Error ? error.message : '请检查 API 会话。', 'danger')
  }
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="OVERVIEW" title="早上好，开始今天的发布工作" description="从数据来源到公开页面，集中查看需要处理的内容、产品与询盘。">
      <template #actions>
        <RouterLink class="button button--secondary" to="/content"><Eye :size="16" />查看内容</RouterLink>
        <RouterLink class="button button--primary" to="/content/new/edit"><BookOpenText :size="16" />新建内容</RouterLink>
      </template>
    </PageHeader>

    <div v-if="mockApiEnabled" class="demo-banner">
      <span>开发演示数据</span>
      <p>以下数量和记录仅用于验证界面，不代表 AIRTEKPOWER 的实际业务或产品事实。</p>
      <button type="button" @click="acknowledge">知道了</button>
    </div>

    <section class="metric-grid" aria-label="关键指标">
      <MetricCard label="待发布内容" :value="metricValue('drafts')" :detail="hasMore.drafts ? '前 100 条中的草稿、计划或占位记录' : '草稿、计划或占位记录'" :icon="BookOpenText" tone="blue" />
      <MetricCard label="产品数据冲突" :value="metricValue('conflicts')" :detail="hasMore.conflicts ? '前 100 条；阻止下一次产品发布' : '阻止下一次产品发布'" :icon="Boxes" tone="amber" />
      <MetricCard label="RFQ 记录" :value="String(metrics.rfqs)" detail="按 PII 权限隔离" :icon="Inbox" tone="green" />
      <MetricCard label="运行中任务" :value="metricValue('operations')" :detail="hasMore.operations ? '前 100 条中排队或执行中的任务' : '排队或执行中'" :icon="RefreshCw" tone="slate" />
    </section>

    <section class="dashboard-grid">
      <article class="panel performance-panel">
        <header class="panel__header">
          <div><p class="eyebrow">SITE SIGNAL</p><h2>公开站关键行为</h2></div>
          <select aria-label="选择分析时段"><option>最近 7 天</option><option>最近 30 天</option></select>
        </header>
        <div class="performance-summary">
          <div><span>已接受行为事件</span><strong>{{ metrics.events }}</strong><em><TrendingUp :size="13" />第一方白名单</em></div>
          <div><span>产品互动</span><strong>{{ mockApiEnabled ? 386 : '—' }}</strong><small>聚合维度适配器待接入</small></div>
          <div><span>RFQ 记录</span><strong>{{ metrics.rfqs }}</strong><small>不含任何表单正文或 PII</small></div>
        </div>
        <div v-if="mockApiEnabled" class="signal-chart" aria-label="最近七天行为趋势示意图">
          <div v-for="(height, index) in [42, 57, 50, 72, 62, 80, 74]" :key="index" class="signal-chart__day">
            <span :style="{ height: `${height}%` }"><i :style="{ height: `${Math.round(height * 0.48)}%` }"></i></span>
            <small>{{ ['周一', '周二', '周三', '周四', '周五', '周六', '今天'][index] }}</small>
          </div>
        </div>
        <p v-else class="empty-mini">时间序列聚合端点尚未返回数据；不会用示意柱替代真实趋势。</p>
        <footer class="panel__footer"><span><i class="legend-dot legend-dot--blue"></i>内容互动</span><span><i class="legend-dot legend-dot--green"></i>高意向操作</span><RouterLink to="/analytics">打开 Analytics <ArrowRight :size="14" /></RouterLink></footer>
      </article>

      <article class="panel task-panel">
        <header class="panel__header"><div><p class="eyebrow">PRIORITY</p><h2>需要处理</h2></div><ListTodo :size="20" /></header>
        <RouterLink v-for="task in tasks" :key="task.title" :to="task.to" class="task-row">
          <span class="task-row__icon" :class="`task-row__icon--${task.tone}`"><component :is="task.icon" :size="18" /></span>
          <span><strong>{{ task.title }}</strong><small>{{ task.detail }}</small></span>
          <ArrowRight :size="15" />
        </RouterLink>
        <footer class="panel__footer panel__footer--between"><span>按风险与时效排序</span><button type="button">刷新</button></footer>
      </article>
    </section>

    <section class="dashboard-grid dashboard-grid--bottom">
      <article class="panel activity-panel">
        <header class="panel__header"><div><p class="eyebrow">AUDIT TRAIL</p><h2>最近活动</h2></div><RouterLink to="/audit">全部日志</RouterLink></header>
        <div class="activity-list">
          <div v-for="activity in activities" :key="`${activity.action}-${activity.when}`" class="activity-row">
            <span class="activity-row__icon" :class="`activity-row__icon--${activity.tone}`"><component :is="activity.icon" :size="17" /></span>
            <div><strong>{{ activity.action }}</strong><p>{{ activity.actor }} · {{ activity.when }}</p></div>
          </div>
          <p v-if="!activities.length" class="empty-mini">审计 API 暂无最近活动。</p>
        </div>
      </article>

      <article class="panel readiness-panel">
        <header class="panel__header"><div><p class="eyebrow">PUBLISHING READINESS</p><h2>发布准备度</h2></div><StatusBadge label="需要处理" tone="warning" /></header>
        <div class="readiness-score"><strong>{{ mockApiEnabled ? 74 : '—' }}</strong><span>/ 100</span><svg viewBox="0 0 120 120" aria-hidden="true"><circle cx="60" cy="60" r="52" /><circle class="progress" cx="60" cy="60" r="52" /></svg></div>
        <ul class="readiness-list">
          <li><CheckCircle2 :size="16" />路由与 SEO 基础已就绪</li>
          <li><CircleAlert :size="16" />{{ metricValue('conflicts') }} 个产品来源冲突</li>
          <li><MousePointerClick :size="16" />{{ metricValue('drafts') }} 个内容记录待处理</li>
        </ul>
        <RouterLink class="button button--secondary button--wide" to="/operations">查看发布检查</RouterLink>
      </article>
    </section>
  </div>
</template>
