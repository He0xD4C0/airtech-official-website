<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { AlertTriangle, Cable, Clock3, Database, Play, RefreshCw, RotateCcw, Save } from 'lucide-vue-next'
import type {
  FeishuConnectionStatus,
  FeishuConnectionTest,
  FeishuSettings,
  FeishuSyncRunDetail,
  SyncRunKind,
  UpdateFeishuSettings,
} from '@airtek/contracts'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendSyncRun } from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const connection = ref<FeishuConnectionStatus | null>(null)
const settings = ref<FeishuSettings | null>(null)
const draft = ref<UpdateFeishuSettings | null>(null)
const connectionTest = ref<FeishuConnectionTest | null>(null)
const selectedDetail = ref<FeishuSyncRunDetail | null>(null)
const pageError = ref('')
const saving = ref(false)
const testing = ref(false)
const starting = ref(false)
const loadingDetail = ref(false)
const rollingBack = ref(false)
const runKind = ref<SyncRunKind>('incremental')

const runPager = useCursorPagination<BackendSyncRun>(adminApi.listSyncRuns, {
  errorMessage: '无法读取飞书同步运行记录。',
  onError: (message) => ui.toast('同步记录读取失败', message, 'danger'),
})
const runs = computed(() => runPager.items.value)
const runsState = computed(() => {
  if (runPager.loading.value && !runs.value.length) return 'loading'
  if (runPager.errorStatus.value === 403) return 'forbidden'
  if (runPager.error.value) return 'error'
  return runs.value.length ? 'ready' : 'empty'
})
const latestRun = computed(() => runs.value[0] ?? connection.value?.latestSync ?? null)
const canRollback = computed(() => Boolean(
  selectedDetail.value
  && ['completed', 'completedWithErrors'].includes(selectedDetail.value.run.status)
  && selectedDetail.value.run.recordsApplied > 0,
))

function formatDate(value: string | null | undefined): string {
  return value ? new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(value)) : '尚无记录'
}

function runTone(status: BackendSyncRun['status']): 'success' | 'danger' | 'warning' | 'info' {
  if (status === 'completed') return 'success'
  if (status === 'failed') return 'danger'
  if (status === 'completedWithErrors') return 'warning'
  return 'info'
}

function editSettings(value: FeishuSettings): void {
  draft.value = {
    enabled: value.enabled,
    intervalMinutes: value.intervalMinutes,
    fullReconcileEnabled: value.fullReconcileEnabled,
    fullReconcileLocalTime: value.fullReconcileLocalTime,
  }
}

async function loadSettings(): Promise<void> {
  const result = await adminApi.getFeishuSettings()
  settings.value = result.settings
  editSettings(result.settings)
}

async function refreshPage(): Promise<void> {
  pageError.value = ''
  const results = await Promise.allSettled([
    loadSettings(),
    adminApi.connectionStatus().then((value) => { connection.value = value }),
    runPager.first(),
  ])
  const rejected = results.find((result) => result.status === 'rejected')
  if (rejected?.status === 'rejected') {
    pageError.value = apiErrorMessage(rejected.reason, '飞书同步设置暂时不可用。')
  }
}

async function saveSettings(): Promise<void> {
  if (!settings.value || !draft.value || saving.value) return
  saving.value = true
  try {
    const result = await adminApi.updateFeishuSettings(settings.value.revision, draft.value)
    settings.value = result.settings
    editSettings(result.settings)
    await Promise.all([runPager.refresh(), adminApi.connectionStatus().then((value) => { connection.value = value })])
    ui.toast('调度设置已保存', draft.value.enabled ? '自动同步已启用。' : '自动同步已关闭。')
  } catch (error) {
    ui.toast('调度设置保存失败', apiErrorMessage(error, '请刷新后重试。'), 'danger')
  } finally {
    saving.value = false
  }
}

async function testConnection(): Promise<void> {
  if (testing.value) return
  testing.value = true
  try {
    connectionTest.value = await adminApi.testFeishuConnection()
    connection.value = await adminApi.connectionStatus()
    ui.toast(
      connectionTest.value.runnable ? '连接测试通过' : '连接测试未通过',
      connectionTest.value.runnable ? '四张表、字段映射和对象存储均可用。' : '请查看表级错误。',
      connectionTest.value.runnable ? 'success' : 'danger',
    )
  } catch (error) {
    ui.toast('连接测试失败', apiErrorMessage(error, '请检查飞书凭据和对象存储。'), 'danger')
  } finally {
    testing.value = false
  }
}

async function startSync(): Promise<void> {
  if (starting.value) return
  starting.value = true
  try {
    const run = await adminApi.startFeishuSync({ runKind: runKind.value })
    await runPager.refresh()
    await selectRun(run)
    ui.toast('同步已进入队列', `${runKind.value === 'full' ? '全量对账' : '增量同步'}将在后台运行。`)
  } catch (error) {
    ui.toast('无法启动同步', apiErrorMessage(error, '请先通过连接测试。'), 'danger')
  } finally {
    starting.value = false
  }
}

async function selectRun(run: BackendSyncRun): Promise<void> {
  loadingDetail.value = true
  try {
    selectedDetail.value = await adminApi.getFeishuSyncRun(run.id)
  } catch (error) {
    ui.toast('运行详情读取失败', apiErrorMessage(error, '请稍后重试。'), 'danger')
  } finally {
    loadingDetail.value = false
  }
}

async function rollbackSelected(): Promise<void> {
  if (!selectedDetail.value || rollingBack.value) return
  rollingBack.value = true
  try {
    const report = await adminApi.rollbackFeishuSyncRun(selectedDetail.value.run.id)
    ui.toast('回滚完成', `已恢复 ${report.restored} 个产品，跳过 ${report.skipped} 个已被后续运行修改的产品。`)
    await Promise.all([runPager.refresh(), selectRun(selectedDetail.value.run)])
  } catch (error) {
    ui.toast('回滚失败', apiErrorMessage(error, '仅未被后续运行修改的产品可以恢复。'), 'danger')
  } finally {
    rollingBack.value = false
  }
}

onMounted(refreshPage)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FEISHU PRODUCT MASTER" title="飞书单向同步" description="飞书事实字段和源附件经自动校验后直接发布；网站不会回写飞书，也不会生成导出文件。">
      <template #actions><button class="button button--secondary" type="button" @click="refreshPage"><RefreshCw :size="16" />刷新</button></template>
    </PageHeader>

    <DataStatePanel v-if="pageError" state="error" :title="pageError" @retry="refreshPage" />

    <section class="sync-status-grid">
      <article><div class="sync-status-grid__icon"><Cable :size="20" /></div><span>连接状态</span><strong>{{ connection?.runnable ? '可运行' : '未就绪' }}</strong><small>{{ connection?.unavailableReason || connection?.displayName || '等待连接测试' }}</small></article>
      <article><div class="sync-status-grid__icon"><Clock3 :size="20" /></div><span>最近运行</span><strong>{{ formatDate(latestRun?.startedAt) }}</strong><small>{{ latestRun?.status || '等待首次全量同步' }}</small></article>
      <article><div class="sync-status-grid__icon"><Database :size="20" /></div><span>已应用产品</span><strong>{{ latestRun?.recordsApplied ?? 0 }}</strong><small>失败 {{ latestRun?.recordsFailed ?? 0 }} 条</small></article>
      <article><div class="sync-status-grid__icon"><Database :size="20" /></div><span>附件</span><strong>{{ latestRun?.assetsCopied ?? 0 }} 新增 / {{ latestRun?.assetsReused ?? 0 }} 复用</strong><small>失败 {{ latestRun?.assetsFailed ?? 0 }} 个</small></article>
    </section>

    <section class="panel settings-panel">
      <header><div><p class="eyebrow">SCHEDULE</p><h2>自动同步设置</h2></div><StatusBadge :label="draft?.enabled ? '已启用' : '已关闭'" :tone="draft?.enabled ? 'success' : 'neutral'" /></header>
      <div v-if="draft" class="settings-grid">
        <label class="field field--check"><input v-model="draft.enabled" type="checkbox" /><span>启用自动同步</span></label>
        <label class="field"><span>增量间隔（5–1440 分钟）</span><input v-model.number="draft.intervalMinutes" type="number" min="5" max="1440" /></label>
        <label class="field field--check"><input v-model="draft.fullReconcileEnabled" type="checkbox" /><span>启用夜间全量对账</span></label>
        <label class="field"><span>全量时间（Asia/Shanghai）</span><input v-model="draft.fullReconcileLocalTime" type="time" :disabled="!draft.fullReconcileEnabled" /></label>
      </div>
      <div class="button-row"><button class="button button--primary" type="button" :disabled="!draft || saving" @click="saveSettings"><Save :size="16" />保存设置</button><button class="button button--secondary" type="button" :disabled="testing" @click="testConnection"><Cable :size="16" />{{ testing ? '测试中…' : '连接测试' }}</button></div>
      <div v-if="connectionTest" class="data-table-wrap"><table class="data-table"><thead><tr><th>数据表</th><th>访问</th><th>字段</th><th>Mapping</th><th>错误</th></tr></thead><tbody><tr v-for="table in connectionTest.tables" :key="table.tableId"><td><strong>{{ table.name }}</strong><br><code>{{ table.tableId }}</code></td><td><StatusBadge :label="table.accessible ? '可读取' : '失败'" :tone="table.accessible ? 'success' : 'danger'" /></td><td>{{ table.fieldCount }}</td><td><StatusBadge :label="table.mappingValid ? '有效' : '无效'" :tone="table.mappingValid ? 'success' : 'danger'" /></td><td>{{ table.errors.join('；') || '—' }}</td></tr></tbody></table></div>
    </section>

    <section class="panel sync-panel">
      <header class="run-header"><div><p class="eyebrow">SYNC RUNS</p><h2>运行日志与错误隔离</h2></div><div class="run-actions"><select v-model="runKind" aria-label="同步类型"><option value="incremental">增量同步</option><option value="full">全量对账</option></select><button class="button button--primary" type="button" :disabled="starting || !connection?.configured" @click="startSync"><Play :size="16" />{{ starting ? '正在入队…' : '立即同步' }}</button></div></header>
      <DataStatePanel v-if="runsState !== 'ready'" :state="runsState" :title="runsState === 'empty' ? '尚无同步运行' : runPager.error.value || ''" @retry="runPager.refresh" />
      <div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>开始时间</th><th>类型</th><th>状态</th><th>读取 / 应用 / 失败</th><th>附件</th><th></th></tr></thead><tbody><tr v-for="run in runs" :key="run.id"><td>{{ formatDate(run.startedAt) }}<br><code>{{ run.id }}</code></td><td>{{ run.runKind === 'full' ? '全量' : '增量' }}</td><td><StatusBadge :label="run.status" :tone="runTone(run.status)" /></td><td>{{ run.recordsSeen }} / {{ run.recordsApplied }} / {{ run.recordsFailed }}</td><td>{{ run.assetsCopied }} 新增，{{ run.assetsReused }} 复用</td><td><button class="button button--secondary" type="button" :disabled="loadingDetail" @click="selectRun(run)">详情</button></td></tr></tbody></table></div>
      <CursorPaginationControls v-if="runsState === 'ready'" :item-count="runs.length" :page-number="runPager.pageNumber.value" :can-previous="runPager.canPrevious.value" :can-next="runPager.canNext.value" :loading="runPager.loading.value" label="条同步运行" @previous="runPager.previous" @next="runPager.next" />
      <div v-if="selectedDetail" class="run-detail"><header><div><p class="eyebrow">RUN DETAIL</p><h3>{{ selectedDetail.run.id }}</h3></div><button class="button button--secondary" type="button" :disabled="!canRollback || rollingBack" @click="rollbackSelected"><RotateCcw :size="16" />回滚此运行</button></header><p v-if="!selectedDetail.errors.length" class="muted">该运行没有校验或附件错误。</p><div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>记录</th><th>错误代码</th><th>字段</th><th>详情</th><th>时间</th></tr></thead><tbody><tr v-for="error in selectedDetail.errors" :key="`${error.createdAt}-${error.code}-${error.sourceRecordId}`"><td><code>{{ error.sourceRecordId || '表级' }}</code></td><td><span class="error-code"><AlertTriangle :size="14" />{{ error.code }}</span></td><td>{{ error.fieldPath || '—' }}</td><td>{{ error.message }}</td><td>{{ formatDate(error.createdAt) }}</td></tr></tbody></table></div></div>
    </section>
  </div>
</template>

<style scoped>
@layer components {
.settings-panel,.sync-panel { display: grid; gap: 1rem; }
.settings-panel > header,.run-header,.run-detail > header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.settings-grid { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 1rem; }
.field--check { display: flex; flex-direction: row; align-items: center; gap: .6rem; }
.field--check input { width: auto; }
.button-row,.run-actions { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
.run-actions select { min-height: 2.5rem; }
.run-detail { border-top: 1px solid var(--color-border); padding-top: 1rem; display: grid; gap: .8rem; }
.run-detail h3 { font-family: monospace; font-size: .9rem; }
.error-code { display: inline-flex; align-items: center; gap: .3rem; color: var(--color-danger); }
.muted { color: var(--color-text-muted); }
@media (max-width: 760px) { .settings-grid { grid-template-columns: 1fr; } .settings-panel > header,.run-header,.run-detail > header { align-items: flex-start; flex-direction: column; } }
}
</style>
