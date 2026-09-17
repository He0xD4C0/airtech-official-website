<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Download, History, Search, ShieldCheck, X } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi } from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const route = useRoute()
const router = useRouter()
const ui = useUiStore()
const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const actor = ref(typeof route.query.actor === 'string' ? route.query.actor : '')
const action = ref(typeof route.query.action === 'string' ? route.query.action : '')
const resourceType = ref(typeof route.query.resourceType === 'string' ? route.query.resourceType : '')
const from = ref(typeof route.query.from === 'string' ? route.query.from : '')
const to = ref(typeof route.query.to === 'string' ? route.query.to : '')
const total = ref(0)
const exporting = ref(false)
const expandedEventId = ref<string | null>(null)
let filterTimer: ReturnType<typeof setTimeout> | undefined

function filters() {
  return {
    q: q.value || undefined,
    actor: actor.value || undefined,
    action: action.value || undefined,
    resourceType: resourceType.value || undefined,
    from: from.value ? new Date(from.value).toISOString() : undefined,
    to: to.value ? new Date(to.value).toISOString() : undefined,
  }
}

const auditPager = useCursorPagination(async (pagination) => {
  const page = await adminApi.listAudit({ ...pagination, ...filters() })
  total.value = page.total
  return page
}, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('审计日志读取失败', message, 'danger'),
})
const events = computed(() => auditPager.items.value)
const hasFilters = computed(() => Boolean(
  q.value || actor.value || action.value || resourceType.value || from.value || to.value,
))
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (auditPager.loading.value && !events.value.length) return 'loading'
  if (auditPager.errorStatus.value === 403) return 'forbidden'
  if (auditPager.error.value) return 'error'
  return events.value.length ? 'ready' : 'empty'
})

function scheduleFilter(): void {
  if (filterTimer) clearTimeout(filterTimer)
  filterTimer = setTimeout(() => {
    void router.replace({ query: {
      ...(q.value ? { q: q.value } : {}),
      ...(actor.value ? { actor: actor.value } : {}),
      ...(action.value ? { action: action.value } : {}),
      ...(resourceType.value ? { resourceType: resourceType.value } : {}),
      ...(from.value ? { from: from.value } : {}),
      ...(to.value ? { to: to.value } : {}),
    } })
    void auditPager.first()
  }, 250)
}

function clearFilters(): void {
  q.value = ''
  actor.value = ''
  action.value = ''
  resourceType.value = ''
  from.value = ''
  to.value = ''
}

async function exportCsv(): Promise<void> {
  exporting.value = true
  try {
    const blob = await adminApi.exportAudit(filters())
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = 'airtek-audit.csv'
    anchor.click()
    URL.revokeObjectURL(url)
    ui.toast('CSV 已导出', '导出内容使用当前服务端筛选条件，未声明签名。')
  } catch (error) {
    ui.toast('导出失败', apiErrorMessage(error, '请重试。'), 'danger')
  } finally {
    exporting.value = false
  }
}

function toggleDetails(eventId: string): void {
  expandedEventId.value = expandedEventId.value === eventId ? null : eventId
}

function formatPayload(value: unknown): string {
  return value === null || value === undefined ? '无记录' : JSON.stringify(value, null, 2)
}

onMounted(auditPager.first)
watch([q, actor, action, resourceType, from, to], scheduleFilter)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="IMMUTABLE HISTORY" title="审计日志" description="按操作者、动作、资源、时间和全文条件筛选；日志不作为产品事实来源。">
      <template #actions><button class="button button--secondary" type="button" :disabled="exporting" @click="exportCsv"><Download :size="16" />{{ exporting ? '导出中…' : '导出 CSV' }}</button></template>
    </PageHeader>

    <section class="audit-summary"><div><ShieldCheck :size="20" /><span><strong>审计记录只追加</strong><small>当前服务端筛选命中 {{ total }} 条</small></span></div><StatusBadge label="Append only" tone="success" /></section>

    <section class="panel audit-filters" aria-label="审计筛选">
      <label class="search-field"><Search :size="17" /><input v-model="q" aria-label="全文搜索" placeholder="全文搜索操作者、动作、资源、Request ID、原因" /></label>
      <label class="field"><span>操作者</span><input v-model="actor" /></label>
      <label class="field"><span>动作</span><input v-model="action" /></label>
      <label class="field"><span>资源类型</span><input v-model="resourceType" /></label>
      <label class="field"><span>开始时间</span><input v-model="from" type="datetime-local" /></label>
      <label class="field"><span>结束时间（不含）</span><input v-model="to" type="datetime-local" /></label>
      <button class="button button--quiet audit-filters__clear" type="button" :disabled="!hasFilters" @click="clearFilters"><X :size="15" />清空筛选</button>
    </section>

    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '暂无符合条件的审计记录' : state === 'error' ? auditPager.error.value || '' : ''" @retry="auditPager.refresh" />

    <section v-else class="panel table-panel">
      <div class="data-table-wrap"><table class="data-table audit-table"><thead><tr><th>事件</th><th>操作者</th><th>资源</th><th>原因</th><th>Request ID</th><th>时间</th><th aria-label="操作"></th></tr></thead><tbody><template v-for="event in events" :key="event.id"><tr><td><span class="event-name"><History :size="15" /><code>{{ event.action }}</code></span></td><td>{{ event.actor }}</td><td><code>{{ event.entityType }}/{{ event.entityId || 'global' }}</code></td><td>{{ event.reason || '—' }}</td><td><code>{{ event.requestId }}</code></td><td>{{ new Date(event.occurredAt).toLocaleString('zh-CN') }}</td><td><button class="button button--quiet" type="button" :aria-expanded="expandedEventId === event.id" :aria-controls="`audit-detail-${event.id}`" @click="toggleDetails(event.id)">{{ expandedEventId === event.id ? '收起' : '查看详情' }}</button></td></tr><tr v-if="expandedEventId === event.id" class="audit-detail-row"><td colspan="7"><section :id="`audit-detail-${event.id}`" class="audit-detail" :aria-label="`${event.action} 变更详情`"><header><strong>{{ event.action }}</strong><span>Revision {{ event.currentVersion ?? '—' }}</span></header><div><article><h3>变更前</h3><pre>{{ formatPayload(event.before) }}</pre></article><article><h3>变更后</h3><pre>{{ formatPayload(event.after) }}</pre></article></div></section></td></tr></template></tbody></table></div>
      <CursorPaginationControls :item-count="events.length" :page-number="auditPager.pageNumber.value" :can-previous="auditPager.canPrevious.value" :can-next="auditPager.canNext.value" :loading="auditPager.loading.value" :label="`条记录，共 ${total} 条`" @previous="auditPager.previous" @next="auditPager.next" />
    </section>
  </div>
</template>

<style scoped>
@layer components {
.audit-filters { display: grid; grid-template-columns: minmax(260px, 2fr) repeat(3, minmax(140px, 1fr)); gap: .75rem; align-items: end; }
.audit-filters .search-field { grid-column: span 2; }
.audit-filters__clear { min-height: 2.75rem; justify-self: start; }
.audit-detail-row > td { padding: 0; background: color-mix(in srgb, var(--surface-panel, #fff) 92%, var(--airtek-green)); }
.audit-detail { display: grid; gap: .75rem; padding: 1rem; }
.audit-detail > header { display: flex; justify-content: space-between; gap: 1rem; align-items: center; }
.audit-detail > header span { color: var(--text-secondary, #556663); font-size: .78rem; }
.audit-detail > div { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .75rem; }
.audit-detail article { min-width: 0; }
.audit-detail h3 { margin: 0 0 .35rem; font-size: .75rem; color: var(--text-secondary, #556663); }
.audit-detail pre { max-height: 280px; margin: 0; overflow: auto; padding: .75rem; border: 1px solid var(--border-default); border-radius: 8px; background: white; font-size: 0.75rem; white-space: pre-wrap; overflow-wrap: anywhere; }
}
</style>
