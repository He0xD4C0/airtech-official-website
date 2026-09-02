<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { AlertTriangle, ArrowRight, Braces, Cable, Clock3, RefreshCw, Settings2, ShieldAlert } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendSyncConflict, type BackendSyncRun } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const activeTab = ref<'staging' | 'conflicts' | 'mapping' | 'runs'>('conflicts')
const dryRunActive = ref(false)
const selectedConflictId = ref('')

const syncRunPager = useCursorPagination<BackendSyncRun>(adminApi.listSyncRuns, {
  errorMessage: '无法读取同步运行记录。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const conflictPager = useCursorPagination<BackendSyncConflict>(adminApi.listConflicts, {
  errorMessage: '无法读取同步冲突。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const syncRuns = computed(() => syncRunPager.items.value)
const conflicts = computed(() => conflictPager.items.value)
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

function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '未提供'
  return typeof value === 'string' ? value : JSON.stringify(value)
}

const tabs = computed(() => [
  { id: 'staging', label: 'Staging', count: '' },
  { id: 'conflicts', label: '冲突', count: `${conflicts.value.length}${conflictPager.canNext.value ? '+' : ''}` },
  { id: 'mapping', label: '字段映射', count: '' },
  { id: 'runs', label: '运行记录', count: syncRuns.value.length },
] as const)

async function runDryRun(): Promise<void> {
  dryRunActive.value = true
  try {
    const run = await adminApi.startSync(true)
    syncRunPager.items.value = [run, ...syncRunPager.items.value]
    ui.toast('Dry-run 已加入队列', `Operation ${run.id}`, 'info')
  } catch (error) {
    ui.toast('Dry-run 创建失败', error instanceof Error ? error.message : '请检查同步配置。', 'danger')
  } finally {
    dryRunActive.value = false
  }
}

onMounted(async () => {
  await Promise.all([syncRunPager.first(), conflictPager.first()])
  selectedConflictId.value = conflicts.value[0]?.id ?? ''
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FEISHU PRODUCT MASTER" title="同步与冲突" description="产品主数据先进入 Staging，经校验与三方差异处理后才能形成可发布 revision。">
      <template #actions>
        <button class="button button--secondary" type="button"><Settings2 :size="16" />连接设置</button>
        <button class="button button--primary" type="button" :disabled="dryRunActive" @click="runDryRun"><RefreshCw :size="16" :class="{ spin: dryRunActive }" />{{ dryRunActive ? '正在创建…' : '运行 dry-run' }}</button>
      </template>
    </PageHeader>

    <section class="sync-status-grid">
      <article><div class="sync-status-grid__icon"><Cable :size="20" /></div><span>连接状态</span><strong>由后端配置管理</strong><small>浏览器不读取集成凭据</small></article>
      <article><div class="sync-status-grid__icon"><Clock3 :size="20" /></div><span>最近同步</span><strong>{{ syncRuns[0] ? new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(syncRuns[0].startedAt)) : '尚无记录' }}</strong><small>{{ syncRuns[0]?.status || '等待首次运行' }}</small></article>
      <article><div class="sync-status-grid__icon sync-status-grid__icon--warning"><ShieldAlert :size="20" /></div><span>阻塞冲突</span><strong>{{ `${conflicts.length}${conflictPager.canNext.value ? '+' : ''}` }}</strong><small>{{ conflictPager.canNext.value ? '当前页计数；未解决前不可再次发布' : '未解决前不可再次发布' }}</small></article>
      <article><div class="sync-status-grid__icon"><Braces :size="20" /></div><span>Mapping version</span><strong>{{ syncRuns[0]?.mappingVersion || '未提供' }}</strong><small>取自最近一次同步运行</small></article>
    </section>

    <section class="panel sync-panel">
      <div class="tab-bar" role="tablist" aria-label="同步数据视图">
        <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="activeTab === tab.id" :class="{ 'is-active': activeTab === tab.id }" @click="activeTab = tab.id">
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
          <header><div><p class="eyebrow">BLOCKING</p><h2>字段冲突</h2></div><StatusBadge :label="`${conflicts.length} 个待处理`" tone="danger" /></header>
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
          <div class="diff-warning"><ShieldAlert :size="18" /><div><strong>不要通过多数值推断事实</strong><p>必须由 Product Master 或受控工程记录解决；当前 API 尚未提供冲突处理 mutation。</p></div></div>
        </div>
      </div>

      <DataStatePanel v-else-if="activeTab === 'staging'" state="empty" title="Staging 列表端点尚未提供" description="执行 dry-run 会创建真实同步运行；Staging 永远不会直接成为公开数据。" action-label="运行 dry-run" @action="runDryRun" />
      <DataStatePanel v-else-if="activeTab === 'mapping'" state="empty" title="字段映射端点尚未提供" description="后台不会以硬编码映射替代后端的版本化配置。" />
      <div v-else class="mapping-view sync-runs-view">
        <header><div><p class="eyebrow">SYNC RUNS</p><h2>同步运行记录</h2></div><button class="button button--secondary" type="button">查看 Worker 状态</button></header>
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
