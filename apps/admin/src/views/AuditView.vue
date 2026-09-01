<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { CalendarDays, Download, Filter, History, Search, ShieldCheck } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const demoEvents = [
  ['audit-demo-001', 'content.revision.publish_attempt', 'Demo Publisher', 'content/home', 'blocked', 'request-demo-9f1', '今天 10:42'],
  ['audit-demo-002', 'integration.sync.dry_run', 'Demo Integration Operator', 'feishu/mapping-v1', 'success', 'request-demo-9e8', '今天 10:18'],
  ['audit-demo-003', 'product.override.create', 'Demo Product Manager', 'demo-product-002', 'warning', 'request-demo-9a3', '今天 09:56'],
  ['audit-demo-004', 'operation.backup.preflight', 'Demo Super Admin', 'database/primary', 'success', 'request-demo-8c4', '昨天 18:42'],
]

const auditPager = useCursorPagination(async (pagination) => {
  const page = await adminApi.listAudit(pagination)
  return {
    ...page,
    items: page.items.map((event) => [
      event.id,
      event.action,
      event.actor,
      `${event.entityType}/${event.entityId || 'global'}`,
      'recorded',
      event.requestId,
      new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(event.occurredAt)),
    ]),
  }
}, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('审计日志读取失败', message, 'danger'),
})
const events = computed(() => mockApiEnabled ? demoEvents : auditPager.items.value)

onMounted(async () => {
  if (mockApiEnabled) return
  await auditPager.first()
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="IMMUTABLE HISTORY" title="审计日志" description="追踪操作者、资源、变更前后、原因、Request ID 与时间；日志不作为产品事实来源。">
      <template #actions><button class="button button--secondary" type="button"><Download :size="16" />导出签名记录</button></template>
    </PageHeader>

    <section class="audit-summary"><div><ShieldCheck :size="20" /><span><strong>审计记录只追加</strong><small>{{ mockApiEnabled ? '开发演示 · 不代表真实活动' : '记录来自 Rust Audit API' }}</small></span></div><StatusBadge label="Append only" tone="success" /></section>

    <section class="panel table-panel">
      <div class="table-toolbar"><label class="search-field"><Search :size="17" /><input placeholder="搜索事件、资源或 Request ID" /></label><div class="table-toolbar__filters"><button class="button button--quiet" type="button"><CalendarDays :size="15" />最近 7 天</button><button class="button button--quiet" type="button"><Filter :size="15" />筛选</button></div></div>
      <div class="data-table-wrap"><table class="data-table audit-table"><thead><tr><th>事件</th><th>操作者</th><th>资源</th><th>结果</th><th>Request ID</th><th>时间</th></tr></thead><tbody><tr v-for="event in events" :key="event[0]"><td><span class="event-name"><History :size="15" /><code>{{ event[1] }}</code></span></td><td>{{ event[2] }}</td><td><code>{{ event[3] }}</code></td><td><StatusBadge :label="event[4]" :tone="event[4] === 'success' || event[4] === 'recorded' ? 'success' : event[4] === 'blocked' ? 'danger' : 'warning'" /></td><td><code>{{ event[5] }}</code></td><td>{{ event[6] }}</td></tr></tbody></table></div>
      <CursorPaginationControls
        :item-count="events.length"
        :page-number="auditPager.pageNumber.value"
        :can-previous="!mockApiEnabled && auditPager.canPrevious.value"
        :can-next="!mockApiEnabled && auditPager.canNext.value"
        :loading="auditPager.loading.value"
        :label="mockApiEnabled ? '条开发演示审计记录（不含真实用户和业务数据）' : '条持久化审计记录'"
        @previous="auditPager.previous"
        @next="auditPager.next"
      />
    </section>
  </div>
</template>
