<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { AlertTriangle, ArrowRight, Braces, Cable, Clock3, ShieldAlert } from 'lucide-vue-next'
import type { FeishuConnectionStatus, StagingRecord, SyncMapping } from '@airtek/contracts'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendSyncConflict, type BackendSyncRun } from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
type SyncTab = 'staging' | 'conflicts' | 'mapping' | 'runs'
const tabIds: SyncTab[] = ['staging', 'conflicts', 'mapping', 'runs']
const route = useRoute()
const router = useRouter()
const activeTab = ref<SyncTab>('conflicts')
const selectedConflictId = ref('')
const connection = ref<FeishuConnectionStatus | null>(null)
const connectionError = ref('')
const stagingTotal = ref(0)
const conflictTotal = ref(0)
const resolutionReason = ref('Resolve verified Product Master synchronization conflict')
const evidenceReference = ref('')
const overrideExpiry = ref('')
const resolving = ref(false)

const syncRunPager = useCursorPagination<BackendSyncRun>(adminApi.listSyncRuns, {
  errorMessage: '无法读取同步运行记录。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const conflictPager = useCursorPagination<BackendSyncConflict>(adminApi.listConflicts, {
  errorMessage: '无法读取同步冲突。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const stagingPager = useCursorPagination<StagingRecord>(async (pagination) => {
  const page = await adminApi.listStaging(pagination)
  stagingTotal.value = page.total
  return page
}, { errorMessage: '无法读取 Staging 记录。' })
const mappingPager = useCursorPagination<SyncMapping>(adminApi.listMappings, {
  errorMessage: '无法读取版本化字段映射。',
})
const syncRuns = computed(() => syncRunPager.items.value)
const conflicts = computed(() => conflictPager.items.value)
const staging = computed(() => stagingPager.items.value)
const mappings = computed(() => mappingPager.items.value)
const selectedConflict = computed(() => conflicts.value.find((conflict) => conflict.id === selectedConflictId.value) ?? conflicts.value[0])
const selectedDiff = computed(() => selectedConflict.value?.diffs[0])
const conflictState = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (conflictPager.loading.value && !conflicts.value.length) return 'loading'
  if (conflictPager.errorStatus.value === 403) return 'forbidden'
  if (conflictPager.error.value) return 'error'
  return conflicts.value.length ? 'ready' : 'empty'
})
const runsState = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (syncRunPager.loading.value && !syncRuns.value.length) return 'loading'
  if (syncRunPager.errorStatus.value === 403) return 'forbidden'
  if (syncRunPager.error.value) return 'error'
  return syncRuns.value.length ? 'ready' : 'empty'
})
const stagingState = computed(() => stagingPager.loading.value && !staging.value.length ? 'loading' : stagingPager.error.value ? 'error' : staging.value.length ? 'ready' : 'empty')
const mappingState = computed(() => mappingPager.loading.value && !mappings.value.length ? 'loading' : mappingPager.error.value ? 'error' : mappings.value.length ? 'ready' : 'empty')

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '未提供'
  return typeof value === 'string' ? value : JSON.stringify(value)
}

const tabs = computed(() => [
  { id: 'staging', label: 'Staging', count: String(stagingTotal.value) },
  { id: 'conflicts', label: '冲突', count: String(conflictTotal.value) },
  { id: 'mapping', label: '字段映射', count: '' },
  { id: 'runs', label: '运行记录', count: '' },
] as const)

function selectTab(tab: SyncTab): void {
  void router.replace({ query: { ...route.query, tab } })
}

watch(() => route.query.tab, (value) => {
  activeTab.value = typeof value === 'string' && tabIds.includes(value as SyncTab)
    ? value as SyncTab
    : 'conflicts'
}, { immediate: true })

async function resolveConflict(decision: 'acceptIncoming' | 'keepVerifiedLocal'): Promise<void> {
  if (!selectedConflict.value || resolving.value) return
  resolving.value = true
  try {
    const resolved = await adminApi.resolveConflict(selectedConflict.value.id, selectedConflict.value.revision, {
      decision,
      reason: resolutionReason.value,
      evidenceReference: decision === 'keepVerifiedLocal' ? evidenceReference.value : null,
      expiresAt: decision === 'keepVerifiedLocal' && overrideExpiry.value ? new Date(overrideExpiry.value).toISOString() : null,
    })
    conflictPager.items.value = conflictPager.items.value.filter((entry) => entry.id !== resolved.id)
    conflictTotal.value = Math.max(0, conflictTotal.value - 1)
    selectedConflictId.value = conflictPager.items.value[0]?.id ?? ''
    ui.toast('冲突已解决', decision === 'acceptIncoming' ? '已接受 incoming；同步可继续。' : '已按证据建立临时本地覆盖。')
  } catch (error) {
    ui.toast('冲突处理失败', apiErrorMessage(error, '请检查证据、到期日和版本。'), 'danger')
  } finally {
    resolving.value = false
  }
}

onMounted(async () => {
  await Promise.all([
    syncRunPager.first(),
    conflictPager.first().then((loaded) => { if (loaded) conflictTotal.value = conflictPager.items.value.length }),
    stagingPager.first(),
    mappingPager.first(),
    adminApi.connectionStatus().then((value) => { connection.value = value }).catch((error) => { connectionError.value = apiErrorMessage(error, '连接状态不可用。') }),
  ])
  const conflictPage = await adminApi.listConflicts({ limit: 1 })
  conflictTotal.value = conflictPage.total
  selectedConflictId.value = conflicts.value[0]?.id ?? ''
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FEISHU PRODUCT MASTER" title="同步与冲突" description="产品主数据先进入 Staging，经校验与三方差异处理后才能形成可发布 revision。">
      <template #actions>
        <button class="button button--primary" type="button" disabled :title="connection?.unavailableReason || 'Feishu provider adapter is not connected.'"><ShieldAlert :size="16" />同步暂不可用</button>
      </template>
    </PageHeader>

    <div class="security-baseline"><ShieldAlert :size="18" /><div><strong>Feishu 适配器尚未连接</strong><p>{{ connection?.unavailableReason || '同步入口已禁用；现有 Staging、映射、冲突和运行历史仍可查看。' }}</p></div><StatusBadge label="不可运行" tone="warning" /></div>

    <section class="sync-status-grid">
      <article><div class="sync-status-grid__icon"><Cable :size="20" /></div><span>连接状态</span><strong>{{ connectionError || (connection?.runnable ? '可运行' : '适配器未连接') }}</strong><small>{{ connection?.displayName || '浏览器不读取集成凭据' }}</small></article>
      <article><div class="sync-status-grid__icon"><Clock3 :size="20" /></div><span>最近同步</span><strong>{{ syncRuns[0] ? new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(syncRuns[0].startedAt)) : '尚无记录' }}</strong><small>{{ syncRuns[0]?.status || '等待首次运行' }}</small></article>
      <article><div class="sync-status-grid__icon sync-status-grid__icon--warning"><ShieldAlert :size="20" /></div><span>阻塞冲突</span><strong>{{ conflictTotal }}</strong><small>服务端未解决总数</small></article>
      <article><div class="sync-status-grid__icon"><Braces :size="20" /></div><span>Mapping version</span><strong>{{ mappings.find((entry) => entry.active)?.version || '无 active mapping' }}</strong><small>来自版本化 Mapping API</small></article>
    </section>

    <section class="panel sync-panel">
      <div class="tab-bar" role="tablist" aria-label="同步数据视图">
        <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="activeTab === tab.id" :class="{ 'is-active': activeTab === tab.id }" @click="selectTab(tab.id)">
          {{ tab.label }}<span v-if="tab.count">{{ tab.count }}</span>
        </button>
      </div>

      <DataStatePanel
        v-if="activeTab === 'conflicts' && conflictState !== 'ready'"
        :state="conflictState"
        :title="conflictState === 'empty' ? '没有待处理冲突' : conflictState === 'error' ? conflictPager.error.value || '' : ''"
        description="新同步仍会先进入 Staging；空状态不代表 Feishu 已连接。"
        @retry="conflictPager.refresh"
      />

      <div v-else-if="activeTab === 'conflicts'" class="conflict-layout">
        <div class="conflict-list">
          <header><div><p class="eyebrow">BLOCKING</p><h2>字段冲突</h2></div><StatusBadge :label="`${conflictTotal} 个待处理`" tone="danger" /></header>
          <button v-for="conflict in conflicts" :key="conflict.id" type="button" class="conflict-item" :class="{ 'is-active': selectedConflict?.id === conflict.id }" @click="selectedConflictId = conflict.id"><AlertTriangle :size="18" /><span><strong>{{ conflict.sourceRecordId }}</strong><small>{{ conflict.diffs.length }} 个字段差异</small></span><ArrowRight :size="15" /></button>
          <CursorPaginationControls
            :item-count="conflicts.length"
            :page-number="conflictPager.pageNumber.value"
            :can-previous="conflictPager.canPrevious.value"
            :can-next="conflictPager.canNext.value"
            :loading="conflictPager.loading.value"
            label="条冲突记录"
            @previous="conflictPager.previous"
            @next="conflictPager.next"
          />
        </div>

        <div class="diff-view">
          <header><div><p class="eyebrow">THREE-WAY DIFF</p><h2>{{ selectedDiff?.fieldPath || 'Field-level diff' }}</h2><p>值由 Rust API 返回；处理前需确认字段所有权。</p></div></header>
          <div v-if="selectedDiff" class="diff-columns"><article><span>LAST ACCEPTED BASE</span><code>{{ displayValue(selectedDiff.baseValue) }}</code><small>Accepted revision</small></article><article class="diff-columns__local"><span>CURRENT LOCAL</span><code>{{ displayValue(selectedDiff.localValue) }}</code><small>Portal working value</small></article><article class="diff-columns__incoming"><span>INCOMING FEISHU</span><code>{{ displayValue(selectedDiff.incomingValue) }}</code><small>Staging snapshot</small></article></div>
          <div v-else class="module-empty module-empty--compact"><AlertTriangle :size="24" /><h2>该冲突没有字段差异</h2><p>请检查同步记录的数据完整性。</p></div>
          <div class="diff-warning"><ShieldAlert :size="18" /><div><strong>不要通过多数值推断事实</strong><p>只能接受 incoming，或引用受控证据临时保留当前已验证本地值。</p></div></div>
          <label class="field"><span>处理原因（至少 10 字符）</span><textarea v-model="resolutionReason" rows="2" maxlength="2000" /></label>
          <div class="conflict-resolution-actions"><button class="button button--primary" type="button" :disabled="resolving || resolutionReason.trim().length < 10" @click="resolveConflict('acceptIncoming')">接受 Incoming</button><label class="field"><span>证据引用</span><input v-model="evidenceReference" placeholder="受控工程记录、Product Master 记录 ID" /></label><label class="field"><span>临时覆盖到期日</span><input v-model="overrideExpiry" type="datetime-local" /></label><button class="button button--secondary" type="button" :disabled="resolving || resolutionReason.trim().length < 10 || !evidenceReference.trim() || !overrideExpiry" @click="resolveConflict('keepVerifiedLocal')">保留已验证本地值</button></div>
        </div>
      </div>

      <div v-else-if="activeTab === 'staging'" class="mapping-view"><DataStatePanel v-if="stagingState !== 'ready'" :state="stagingState" :title="stagingState === 'empty' ? '尚无 Staging 记录' : stagingPager.error.value || ''" @retry="stagingPager.refresh" /><div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>Source record</th><th>状态</th><th>验证问题</th><th>创建时间</th></tr></thead><tbody><tr v-for="entry in staging" :key="entry.id"><td><code>{{ entry.sourceRecordId }}</code></td><td><StatusBadge :label="entry.validationStatus" :tone="entry.validationStatus === 'valid' ? 'success' : entry.validationStatus === 'invalid' ? 'danger' : 'warning'" /></td><td>{{ entry.validationErrors.length }}</td><td>{{ new Date(entry.createdAt).toLocaleString('zh-CN') }}</td></tr></tbody></table></div><CursorPaginationControls v-if="stagingState === 'ready'" :item-count="staging.length" :page-number="stagingPager.pageNumber.value" :can-previous="stagingPager.canPrevious.value" :can-next="stagingPager.canNext.value" :loading="stagingPager.loading.value" :label="`条 Staging，共 ${stagingTotal} 条`" @previous="stagingPager.previous" @next="stagingPager.next" /></div>
      <div v-else-if="activeTab === 'mapping'" class="mapping-view"><DataStatePanel v-if="mappingState !== 'ready'" :state="mappingState" :title="mappingState === 'empty' ? '尚无版本化 Mapping' : mappingPager.error.value || ''" @retry="mappingPager.refresh" /><div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>版本</th><th>Schema</th><th>状态</th><th>映射</th></tr></thead><tbody><tr v-for="entry in mappings" :key="entry.id"><td><code>{{ entry.version }}</code></td><td>{{ entry.schemaVersion }}</td><td><StatusBadge :label="entry.active ? 'active' : 'inactive'" :tone="entry.active ? 'success' : 'neutral'" /></td><td><code>{{ JSON.stringify(entry.mapping) }}</code></td></tr></tbody></table></div></div>
      <div v-else class="mapping-view sync-runs-view">
        <header><div><p class="eyebrow">SYNC RUNS</p><h2>同步运行记录</h2></div></header>
        <DataStatePanel v-if="runsState !== 'ready'" :state="runsState" :title="runsState === 'empty' ? '尚无同步运行' : runsState === 'error' ? syncRunPager.error.value || '' : ''" @retry="syncRunPager.refresh" />
        <div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>Run ID</th><th>模式</th><th>状态</th><th>已读取</th><th>通过校验</th><th>冲突</th><th>开始时间</th></tr></thead><tbody><tr v-for="run in syncRuns" :key="run.id"><td><code>{{ run.id }}</code></td><td>{{ run.dryRun ? 'Dry-run' : '正式同步' }}</td><td><StatusBadge :label="run.status" :tone="run.status === 'completed' ? 'success' : run.status === 'failed' ? 'danger' : 'info'" /></td><td>{{ run.recordsSeen }}</td><td>{{ run.recordsValid }}</td><td>{{ run.conflictCount }}</td><td>{{ new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(run.startedAt)) }}</td></tr></tbody></table></div>
        <CursorPaginationControls
          v-if="runsState === 'ready'"
          :item-count="syncRuns.length"
          :page-number="syncRunPager.pageNumber.value"
          :can-previous="syncRunPager.canPrevious.value"
          :can-next="syncRunPager.canNext.value"
          :loading="syncRunPager.loading.value"
          label="条同步运行"
          @previous="syncRunPager.previous"
          @next="syncRunPager.next"
        />
      </div>
    </section>
  </div>
</template>

<style scoped>
.conflict-resolution-actions { display: grid; gap: .65rem; margin-top: .75rem; }
</style>
