<script setup lang="ts">
import { inboxApi } from '@/features/inbox/services/inboxApi'
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Building2, CheckCircle2, Clock3, Inbox, LockKeyhole, Mail, Search, UserRound } from 'lucide-vue-next'
import type { BusinessInboxDetail, BusinessInboxItem, BusinessInboxStatus, BusinessPii } from '@airtek/contracts'
import RfqContextDetails from '@/features/inbox/components/RfqContextDetails.vue'
import CursorPaginationControls from '@/shared/components/CursorPaginationControls.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import StatusBadge from '@/shared/components/StatusBadge.vue'
import { useCursorPagination } from '@/shared/composables/useCursorPagination'

import { apiErrorMessage } from '@/shared/services/cursorPagination'
import { useAuthStore } from '@/shared/stores/auth'
import { useUiStore } from '@/shared/stores/ui'

const props = defineProps<{ kind: 'rfq' | 'contact' }>()
const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const selectedId = ref('')
const query = ref(typeof route.query.q === 'string' ? route.query.q : '')
const status = ref<BusinessInboxStatus | ''>(
  typeof route.query.status === 'string' ? route.query.status as BusinessInboxStatus : '',
)
const mineOnly = ref(route.query.mine === '1')
const total = ref(0)
const detail = ref<BusinessInboxDetail | null>(null)
const detailLoading = ref(false)
const pii = ref<BusinessPii | null>(null)
const piiLoading = ref(false)
const mutationLoading = ref(false)
const nextStatus = ref<BusinessInboxStatus>('triaged')
const noteBody = ref('')
const reason = ref('Admin inbox workflow update')
let filterTimer: ReturnType<typeof setTimeout> | undefined

const inboxPager = useCursorPagination(async (pagination) => {
  const request = {
    ...pagination,
    q: query.value || undefined,
    status: status.value || undefined,
    assignedTo: mineOnly.value ? auth.user?.id : undefined,
  }
  const page = props.kind === 'contact'
    ? await inboxApi.listContacts(request)
    : await inboxApi.listRfqs(request)
  total.value = page.total
  return page
}, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('收件箱读取失败', message, 'danger'),
})

const items = computed(() => inboxPager.items.value)
const selected = computed(() => detail.value?.item ?? items.value.find((item) => item.id === selectedId.value) ?? null)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (inboxPager.loading.value && !items.value.length) return 'loading'
  if (inboxPager.errorStatus.value === 403) return 'forbidden'
  if (inboxPager.error.value) return 'error'
  return items.value.length ? 'ready' : 'empty'
})
const canMutate = computed(() => auth.hasPermission('rfq.assign') && selected.value?.status !== 'piiCleared')
const canReadPii = computed(() => auth.hasPermission('rfq.read_pii'))

function kindLabel(item: BusinessInboxItem): string {
  if (item.entityType === 'contact') return item.topic || 'General Contact'
  return `${item.journey || 'project'} RFQ`
}

async function loadDetail(id: string): Promise<void> {
  selectedId.value = id
  detailLoading.value = true
  pii.value = null
  try {
    detail.value = props.kind === 'contact' ? await inboxApi.getContact(id) : await inboxApi.getRfq(id)
    nextStatus.value = detail.value.item.status === 'new' ? 'triaged' : detail.value.item.status
  } catch (error) {
    ui.toast('详情读取失败', apiErrorMessage(error, '请重试。'), 'danger')
  } finally {
    detailLoading.value = false
  }
}

async function loadFirstPage(): Promise<void> {
  if (await inboxPager.first()) {
    const id = inboxPager.items.value[0]?.id
    if (id) await loadDetail(id)
    else detail.value = null
  }
}

async function movePage(direction: 'previous' | 'next'): Promise<void> {
  if (await inboxPager[direction]()) {
    const id = inboxPager.items.value[0]?.id
    if (id) await loadDetail(id)
  }
}

async function loadPii(): Promise<void> {
  if (!selected.value || piiLoading.value) return
  piiLoading.value = true
  try {
    pii.value = props.kind === 'contact'
      ? await inboxApi.getContactPii(selected.value.id)
      : await inboxApi.getRfqPii(selected.value.id)
  } catch (error) {
    ui.toast('PII 读取失败', apiErrorMessage(error, '请检查权限。'), 'danger')
  } finally {
    piiLoading.value = false
  }
}

async function assignToMe(): Promise<void> {
  if (!selected.value || !auth.user || mutationLoading.value) return
  await mutate(async () => props.kind === 'contact'
    ? inboxApi.assignContact(selected.value!.id, selected.value!.revision, auth.user!.id, reason.value)
    : inboxApi.assignRfq(selected.value!.id, selected.value!.revision, auth.user!.id, reason.value), '已分配给当前用户')
}

async function updateStatus(target: BusinessInboxStatus = nextStatus.value): Promise<void> {
  if (!selected.value || mutationLoading.value) return
  await mutate(async () => props.kind === 'contact'
    ? inboxApi.updateContactStatus(selected.value!.id, selected.value!.revision, target, reason.value)
    : inboxApi.updateRfqStatus(selected.value!.id, selected.value!.revision, target, reason.value), target === 'spam' ? '已标记为垃圾' : '状态已更新')
}

async function addNote(): Promise<void> {
  if (!selected.value || !noteBody.value.trim() || mutationLoading.value) return
  mutationLoading.value = true
  try {
    detail.value = props.kind === 'contact'
      ? await inboxApi.addContactNote(selected.value.id, selected.value.revision, noteBody.value, reason.value)
      : await inboxApi.addRfqNote(selected.value.id, selected.value.revision, noteBody.value, reason.value)
    noteBody.value = ''
    replaceListItem(detail.value.item)
    ui.toast('内部备注已添加', '备注不可修改或删除。')
  } catch (error) {
    ui.toast('备注添加失败', apiErrorMessage(error, '请检查版本与权限。'), 'danger')
  } finally {
    mutationLoading.value = false
  }
}

async function mutate(action: () => Promise<BusinessInboxItem>, success: string): Promise<void> {
  mutationLoading.value = true
  try {
    const item = await action()
    replaceListItem(item)
    await loadDetail(item.id)
    ui.toast(success, `当前 revision ${item.revision}。`)
  } catch (error) {
    ui.toast('工作流更新失败', apiErrorMessage(error, '请检查版本、原因与权限。'), 'danger')
  } finally {
    mutationLoading.value = false
  }
}

function replaceListItem(item: BusinessInboxItem): void {
  const index = inboxPager.items.value.findIndex((entry) => entry.id === item.id)
  if (index >= 0) inboxPager.items.value[index] = item
}

function scheduleFilter(): void {
  if (filterTimer) clearTimeout(filterTimer)
  filterTimer = setTimeout(() => {
    void router.replace({ query: {
      ...(query.value ? { q: query.value } : {}),
      ...(status.value ? { status: status.value } : {}),
      ...(mineOnly.value ? { mine: '1' } : {}),
    } })
    void loadFirstPage()
  }, 250)
}

onMounted(loadFirstPage)
watch([query, status, mineOnly], scheduleFilter)
watch(() => props.kind, () => { selectedId.value = ''; detail.value = null; pii.value = null; void loadFirstPage() })
</script>

<template>
  <div class="page-stack">
    <PageHeader :eyebrow="kind === 'rfq' ? 'QUALIFIED INQUIRIES' : 'GENERAL ROUTING'" :title="kind === 'rfq' ? 'RFQ 收件箱' : 'Contact'" description="服务端筛选、受控状态、不可变备注和独立 PII 读取。" />
    <div class="pii-banner"><LockKeyhole :size="17" /><p><strong>列表与详情始终脱敏。</strong>只有点击读取后才请求 PII，响应禁止缓存且读取动作写入审计。</p><span>{{ total }} 条结果</span></div>

    <section class="panel inbox-filters" aria-label="收件箱筛选">
      <label class="search-field"><Search :size="16" /><input v-model="query" placeholder="服务端搜索 reference、组织或提交字段" /></label>
      <label class="field"><span>状态</span><select v-model="status"><option value="">全部状态</option><option v-for="value in ['new','triaged','assigned','qualified','closed','spam']" :key="value" :value="value">{{ value }}</option></select></label>
      <label v-if="auth.user" class="toggle-row"><span>只看分配给我</span><input v-model="mineOnly" type="checkbox" /></label>
    </section>

    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '没有符合筛选条件的记录' : state === 'error' ? inboxPager.error.value || '' : ''" @retry="loadFirstPage" />

    <section v-else class="inbox-layout panel">
      <div class="inbox-list">
        <button v-for="item in items" :key="item.id" type="button" class="inbox-item" :class="{ 'is-active': selected?.id === item.id }" @click="loadDetail(item.id)">
          <span class="inbox-item__type"><Inbox v-if="item.entityType === 'rfq'" :size="17" /><Mail v-else :size="17" /></span>
          <span><strong>{{ item.organization || item.reference }}</strong><small>{{ kindLabel(item) }} · {{ item.countryOrRegion || 'Region not supplied' }}</small><em>{{ new Date(item.updatedAt).toLocaleString('zh-CN') }}</em></span>
          <StatusBadge :label="item.status" :tone="item.status === 'new' ? 'danger' : item.status === 'spam' ? 'neutral' : 'info'" />
        </button>
        <CursorPaginationControls :item-count="items.length" :page-number="inboxPager.pageNumber.value" :can-previous="inboxPager.canPrevious.value" :can-next="inboxPager.canNext.value" :loading="inboxPager.loading.value" :label="`条记录，共 ${total} 条`" @previous="movePage('previous')" @next="movePage('next')" />
      </div>

      <article v-if="selected" class="inbox-detail" :aria-busy="detailLoading">
        <header><div><p class="eyebrow">{{ selected.reference }}</p><h2>{{ selected.organization || 'Organization not supplied' }}</h2><p>{{ kindLabel(selected) }} · revision {{ selected.revision }}</p></div><button v-if="canMutate" class="button button--primary" type="button" :disabled="mutationLoading" @click="assignToMe"><UserRound :size="16" />分配给我</button></header>
        <div class="inbox-detail__meta"><span><Clock3 :size="15" />{{ new Date(selected.submittedAt).toLocaleString('zh-CN') }}</span><span><Building2 :size="15" />{{ selected.countryOrRegion || '未提供地区' }}</span><span><StatusBadge :label="selected.status" tone="info" /></span></div>
        <section class="detail-section"><h3>结构化上下文</h3><dl><div><dt>来源页面</dt><dd><code>{{ selected.sourcePath }}</code></dd></div><div><dt>产品上下文</dt><dd>{{ selected.productContext ? `${selected.productContext.model || selected.productContext.stableId} · revision ${selected.productContext.publishedRevision}` : '未提供' }}</dd></div><div><dt>Consent</dt><dd><CheckCircle2 :size="14" />{{ selected.consent ? '已记录' : '未记录' }}</dd></div><div><dt>负责人</dt><dd>{{ selected.assignedTo || '未分配' }}</dd></div></dl></section>
        <p v-if="selected.status === 'piiCleared'">联系信息和工程需求已按保留期清除，不能再次读取。</p>
        <RfqContextDetails v-if="pii?.rfqContext" :snapshot="pii.rfqContext" />
        <section v-if="selected.status !== 'piiCleared'" class="detail-section"><h3>联系信息与需求（受控读取）</h3><button v-if="canReadPii && !pii" class="button button--secondary" type="button" :disabled="piiLoading" @click="loadPii"><LockKeyhole :size="15" />{{ piiLoading ? '读取中…' : '读取并记录审计' }}</button><dl v-if="pii"><div><dt>姓名</dt><dd>{{ pii.name }}</dd></div><div><dt>邮箱</dt><dd>{{ pii.email }}</dd></div><div><dt>电话</dt><dd>{{ pii.phone || '未提供' }}</dd></div><div v-if="pii.message"><dt>消息</dt><dd>{{ pii.message }}</dd></div></dl><p v-else-if="!canReadPii">当前角色没有 <code>rfq.read_pii</code> 权限。</p></section>
        <section v-if="canMutate" class="detail-section"><h3>工作流</h3><label class="field"><span>变更原因</span><input v-model="reason" minlength="3" maxlength="2000" /></label><div class="inbox-actions"><select v-model="nextStatus"><option v-for="value in ['new','triaged','assigned','qualified','closed','spam']" :key="value" :value="value">{{ value }}</option></select><button class="button button--secondary" type="button" :disabled="mutationLoading || reason.trim().length < 3" @click="updateStatus()">更新状态</button><button class="button button--quiet" type="button" :disabled="mutationLoading || reason.trim().length < 3" @click="updateStatus('spam')">标记垃圾</button></div><label class="field"><span>不可变内部备注</span><textarea v-model="noteBody" rows="3" maxlength="4000" /></label><button class="button button--secondary" type="button" :disabled="mutationLoading || !noteBody.trim() || reason.trim().length < 3" @click="addNote">添加备注</button></section>
        <section v-if="detail?.notes.length" class="detail-section"><h3>内部备注</h3><ol><li v-for="entry in detail.notes" :key="entry.id"><strong>{{ new Date(entry.createdAt).toLocaleString('zh-CN') }}</strong><p>{{ entry.body }}</p></li></ol></section>
        <section v-if="detail?.statusHistory.length" class="detail-section"><h3>状态历史</h3><ol><li v-for="entry in detail.statusHistory" :key="entry.id">{{ entry.fromStatus || 'created' }} → {{ entry.toStatus }} · {{ entry.reason || '无原因' }}</li></ol></section>
      </article>
    </section>
  </div>
</template>

<style scoped>
@layer components {
.inbox-filters { display: grid; grid-template-columns: minmax(260px, 1fr) 180px auto; gap: .75rem; align-items: end; }
.inbox-actions { display: flex; flex-wrap: wrap; gap: .5rem; }
}
</style>
