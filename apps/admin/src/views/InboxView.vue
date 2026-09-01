<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { Building2, CheckCircle2, ChevronRight, Clock3, Filter, Inbox, Mail, MessageSquareText, Search, ShieldCheck, UserRound } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import type { InboxItem } from '@/types/domain'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ kind: 'rfq' | 'contact' }>()
const ui = useUiStore()
const selectedId = ref(mockApiEnabled ? 'demo-rfq-001' : '')
const query = ref('')

const demoItems: InboxItem[] = [
  { id: 'demo-rfq-001', kind: 'product', company: 'Demo Organization A', region: 'Germany', status: 'new', createdAt: '今天 10:24' },
  { id: 'demo-rfq-002', kind: 'selection', company: 'Demo Organization B', region: 'Singapore', status: 'triaged', createdAt: '今天 09:11', assignee: 'Demo Operator' },
  { id: 'demo-rfq-003', kind: 'project', company: 'Demo Organization C', region: 'Canada', status: 'assigned', createdAt: '昨天 16:42', assignee: 'Demo Operator' },
  { id: 'demo-contact-001', kind: 'contact', company: 'Demo Organization D', region: 'United Kingdom', status: 'new', createdAt: '昨天 12:08' },
]
const kindLabel = (kind: InboxItem['kind']) => ({ product: 'Product RFQ', selection: 'Selection RFQ', project: 'Project RFQ', replacement: 'Replacement RFQ', contact: 'General Contact' })[kind]

function assign(): void {
  ui.toast(mockApiEnabled ? '已分配演示记录' : '分配接口尚未启用', mockApiEnabled ? '正式操作将写入状态历史与不可变审计日志。' : '当前 API 只提供只读收件箱，未伪造状态变更。', mockApiEnabled ? 'success' : 'warning')
}

function asObject(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {}
}

const inboxPager = useCursorPagination(async (pagination) => {
    const page = props.kind === 'contact' ? await adminApi.listContacts(pagination) : await adminApi.listRfqs(pagination)
    return {
      ...page,
      items: page.items.map((raw): InboxItem => {
      const record = asObject(raw)
      const request = asObject(record.request)
      const contact = asObject(request.contact)
      const productContext = asObject(request.productContext)
      const kind = props.kind === 'contact' ? 'contact' : String(request.journey || 'project') as InboxItem['kind']
      return {
        id: String(record.id || record.reference || 'unknown'),
        kind,
        company: String(contact.company || 'Organization not supplied'),
        region: String(contact.countryOrRegion || 'Region not supplied'),
        status: String(record.status || 'new') as InboxItem['status'],
        createdAt: new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(String(record.submittedAt))),
        sourcePath: typeof request.sourcePath === 'string' ? request.sourcePath : undefined,
        consent: request.consent === true,
        productContext: typeof productContext.stableId === 'string' && typeof productContext.publishedRevision === 'number' ? {
          stableId: productContext.stableId,
          model: typeof productContext.model === 'string' ? productContext.model : undefined,
          publishedRevision: productContext.publishedRevision,
        } : undefined,
      }
      }),
    }
}, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('收件箱读取失败', message, 'danger'),
})

const items = computed(() => mockApiEnabled ? demoItems : inboxPager.items.value)
const visibleItems = computed(() => {
  const normalizedQuery = query.value.trim().toLowerCase()
  return items.value.filter((item) => {
    const matchesKind = props.kind === 'contact' ? item.kind === 'contact' : item.kind !== 'contact'
    const matchesQuery = !normalizedQuery || `${item.id} ${item.company}`.toLowerCase().includes(normalizedQuery)
    return matchesKind && matchesQuery
  })
})
const selected = computed(() => visibleItems.value.find((item) => item.id === selectedId.value) ?? visibleItems.value[0])

async function loadFirstPage(): Promise<void> {
  if (mockApiEnabled) return
  if (await inboxPager.first()) selectedId.value = inboxPager.items.value[0]?.id ?? ''
}

async function movePage(direction: 'previous' | 'next'): Promise<void> {
  if (await inboxPager[direction]()) selectedId.value = inboxPager.items.value[0]?.id ?? ''
}

onMounted(loadFirstPage)
watch(() => props.kind, () => {
  query.value = ''
  void loadFirstPage()
})
</script>

<template>
  <div class="page-stack">
    <PageHeader :eyebrow="kind === 'rfq' ? 'QUALIFIED INQUIRIES' : 'GENERAL ROUTING'" :title="kind === 'rfq' ? 'RFQ 收件箱' : 'Contact'" :description="kind === 'rfq' ? '处理 Product、Selection、Project 与 Replacement 四类结构化询盘。' : '处理不属于结构化 RFQ 的一般联系请求。'">
      <template #actions><button class="button button--secondary" type="button"><ShieldCheck :size="16" />PII 访问策略</button></template>
    </PageHeader>

    <div class="pii-banner"><ShieldCheck :size="17" /><p><strong>{{ mockApiEnabled ? '开发演示已脱敏。' : 'PII 权限边界已启用。' }}</strong>姓名、邮箱与电话仅向授权角色显示，并按保留策略清除。</p><span>默认 365 天</span></div>

    <section class="inbox-layout panel">
      <div class="inbox-list">
        <div class="inbox-toolbar"><label class="search-field"><Search :size="16" /><input v-model="query" placeholder="搜索当前页 ID 或组织" /></label><button class="icon-button" type="button" aria-label="筛选"><Filter :size="17" /></button></div>
        <button v-for="item in visibleItems" :key="item.id" type="button" class="inbox-item" :class="{ 'is-active': selected?.id === item.id }" @click="selectedId = item.id">
          <span class="inbox-item__type"><Inbox v-if="item.kind !== 'contact'" :size="17" /><Mail v-else :size="17" /></span>
          <span><strong>{{ item.company }}</strong><small>{{ kindLabel(item.kind) }} · {{ item.region }}</small><em>{{ item.createdAt }}</em></span>
          <StatusBadge :label="item.status === 'new' ? '新' : item.status === 'triaged' ? '已分流' : '已分配'" :tone="item.status === 'new' ? 'danger' : item.status === 'triaged' ? 'warning' : 'info'" />
        </button>
        <CursorPaginationControls
          :item-count="visibleItems.length"
          :page-number="inboxPager.pageNumber.value"
          :can-previous="!mockApiEnabled && inboxPager.canPrevious.value"
          :can-next="!mockApiEnabled && inboxPager.canNext.value"
          :loading="inboxPager.loading.value"
          :label="`${kind === 'rfq' ? '条 RFQ' : '条 Contact'}（搜索作用于当前页）`"
          @previous="movePage('previous')"
          @next="movePage('next')"
        />
      </div>

      <article v-if="selected" class="inbox-detail">
        <header><div><p class="eyebrow">{{ selected.id.toUpperCase() }}</p><h2>{{ selected.company }}</h2><p>{{ kindLabel(selected.kind) }} · {{ selected.region }}</p></div><button class="button button--primary" type="button" @click="assign"><UserRound :size="16" />分配给我</button></header>
        <div class="inbox-detail__meta"><span><Clock3 :size="15" />收到于 {{ selected.createdAt }}</span><span><Building2 :size="15" />{{ selected.company }}</span><span><StatusBadge :label="selected.status === 'new' ? '待处理' : '处理中'" :tone="selected.status === 'new' ? 'danger' : 'info'" /></span></div>
        <section class="detail-section"><h3>结构化上下文</h3><dl><div><dt>询盘类型</dt><dd>{{ kindLabel(selected.kind) }}</dd></div><div><dt>产品上下文</dt><dd>{{ selected.productContext ? `${selected.productContext.model || selected.productContext.stableId} · revision ${selected.productContext.publishedRevision}` : '不适用或未提供' }}</dd></div><div><dt>来源页面</dt><dd><code>{{ selected.sourcePath || '未提供' }}</code></dd></div><div><dt>Consent</dt><dd><CheckCircle2 v-if="selected.consent" :size="14" />{{ selected.consent ? '已记录' : '未在列表响应中确认' }}</dd></div></dl></section>
        <section class="detail-section"><h3>提交内容</h3><div class="redacted-message"><MessageSquareText :size="19" /><p>{{ mockApiEnabled ? '开发演示不包含自由文本或个人信息。' : '列表响应不在此处展示姓名、邮箱、电话或自由文本；读取明文需要独立的 PII 权限与详情端点。' }}</p></div></section>
        <section class="detail-section"><h3>状态</h3><div class="timeline"><div><i></i><span><strong>{{ selected.status }}</strong><small>收到于 {{ selected.createdAt }}</small></span></div></div></section>
        <footer><button class="button button--quiet" type="button" @click="assign">标记垃圾</button><button class="button button--secondary" type="button" @click="assign">添加内部备注<ChevronRight :size="15" /></button></footer>
      </article>
    </section>
  </div>
</template>
