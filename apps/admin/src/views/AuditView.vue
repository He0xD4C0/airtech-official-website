<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { CalendarDays, Download, Filter, History, Search, ShieldCheck } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const auditPager = useCursorPagination(async (pagination) => {
  const page = await adminApi.listAudit(pagination)
  return {
    ...page,
    items: page.items.map((event) => ({
      id: event.id,
      action: event.action,
      actor: event.actor,
      resource: `${event.entityType}/${event.entityId || 'global'}`,
      requestId: event.requestId,
      occurredAt: new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(event.occurredAt)),
    })),
  }
}, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('审计日志读取失败', message, 'danger'),
})
const events = computed(() => auditPager.items.value)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (auditPager.loading.value && !events.value.length) return 'loading'
  if (auditPager.errorStatus.value === 403) return 'forbidden'
  if (auditPager.error.value) return 'error'
  return events.value.length ? 'ready' : 'empty'
})

onMounted(async () => {
  await auditPager.first()
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="IMMUTABLE HISTORY" title="审计日志" description="追踪操作者、资源、变更前后、原因、Request ID 与时间；日志不作为产品事实来源。">
      <template #actions><button class="button button--secondary" type="button"><Download :size="16" />导出签名记录</button></template>
    </PageHeader>

    <section class="audit-summary"><div><ShieldCheck :size="20" /><span><strong>审计记录只追加</strong><small>记录来自 Rust Audit API</small></span></div><StatusBadge label="Append only" tone="success" /></section>

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? '暂无审计记录' : state === 'error' ? auditPager.error.value || '' : ''"
      @retry="auditPager.refresh"
    />

    <section v-else class="panel table-panel">
      <div class="table-toolbar"><label class="search-field"><Search :size="17" /><input placeholder="搜索事件、资源或 Request ID" /></label><div class="table-toolbar__filters"><button class="button button--quiet" type="button"><CalendarDays :size="15" />最近 7 天</button><button class="button button--quiet" type="button"><Filter :size="15" />筛选</button></div></div>
      <div class="data-table-wrap"><table class="data-table audit-table"><thead><tr><th>事件</th><th>操作者</th><th>资源</th><th>结果</th><th>Request ID</th><th>时间</th></tr></thead><tbody><tr v-for="event in events" :key="event.id"><td><span class="event-name"><History :size="15" /><code>{{ event.action }}</code></span></td><td>{{ event.actor }}</td><td><code>{{ event.resource }}</code></td><td><StatusBadge label="recorded" tone="success" /></td><td><code>{{ event.requestId }}</code></td><td>{{ event.occurredAt }}</td></tr></tbody></table></div>
      <CursorPaginationControls
        :item-count="events.length"
        :page-number="auditPager.pageNumber.value"
        :can-previous="auditPager.canPrevious.value"
        :can-next="auditPager.canNext.value"
        :loading="auditPager.loading.value"
        label="条持久化审计记录"
        @previous="auditPager.previous"
        @next="auditPager.next"
      />
    </section>
  </div>
</template>
