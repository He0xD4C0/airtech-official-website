<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { AlertTriangle, Cable, Clock3, Database, KeyRound, Play, Plus, RefreshCw, Save, Trash2 } from 'lucide-vue-next'
import type {
  FeishuConnectionStatus,
  FeishuConnectionTest,
  FeishuSettings,
  FeishuSyncRunDetail,
  ProductFamily,
  UpdateFeishuSettings,
} from '@airtek/contracts'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendSyncRun } from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const familyOptions: Array<{ value: ProductFamily, label: string }> = [
  { value: 'centrifugal', label: '离心风机' },
  { value: 'axial', label: '轴流风机' },
  { value: 'crossFlow', label: '贯流风机' },
  { value: 'inlineDuct', label: '管道风机' },
  { value: 'motors', label: '电机' },
]

const auth = useAuthStore()
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
const canManageSettings = computed(() => auth.hasPermission('settings.manage'))
const configurationTested = computed(() => Boolean(
  settings.value
  && settings.value.testedConnectionRevision === settings.value.connectionRevision,
))

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

function formatDate(value: string | null | undefined): string {
  return value
    ? new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(value))
    : '尚无记录'
}

function runTone(status: BackendSyncRun['status']): 'success' | 'danger' | 'warning' | 'info' {
  if (status === 'completed') return 'success'
  if (status === 'failed') return 'danger'
  if (status === 'completedWithErrors') return 'warning'
  return 'info'
}

function editSettings(value: FeishuSettings): void {
  draft.value = {
    appId: value.appId ?? '',
    appSecret: '',
    clearCredentials: false,
    sources: value.sources.map((source) => ({ ...source })),
    enabled: value.enabled,
    intervalEnabled: value.intervalEnabled,
    intervalMinutes: value.intervalMinutes,
    dailyEnabled: value.dailyEnabled,
    dailyLocalTime: value.dailyLocalTime,
  }
}

function addSource(): void {
  draft.value?.sources.push({
    enabled: true,
    name: '',
    wikiToken: '',
    tableId: '',
    family: 'axial',
    application: null,
  })
}

function removeSource(index: number): void {
  draft.value?.sources.splice(index, 1)
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
  if (!settings.value || !draft.value || saving.value || !canManageSettings.value) return
  if (!draft.value.clearCredentials) {
    const changedApp = draft.value.appId.trim() !== (settings.value.appId ?? '')
    if (!draft.value.appId.trim() || ((!settings.value.secretConfigured || changedApp) && !draft.value.appSecret)) {
      ui.toast('飞书凭据不完整', '首次配置或更改 App ID 时必须填写新的 App Secret。', 'warning')
      return
    }
  }
  const connectionChanged = draft.value.clearCredentials
    || Boolean(draft.value.appSecret)
    || draft.value.appId.trim() !== (settings.value.appId ?? '')
    || JSON.stringify(draft.value.sources) !== JSON.stringify(settings.value.sources)
  if (connectionChanged && draft.value.enabled) {
    ui.toast('请先关闭连接器', '修改凭据或来源时先关闭连接器，保存并通过连接测试后再启用。', 'warning')
    return
  }
  saving.value = true
  try {
    const result = await adminApi.updateFeishuSettings(settings.value.revision, draft.value)
    settings.value = result.settings
    editSettings(result.settings)
    if (connectionChanged) connectionTest.value = null
    connection.value = await adminApi.connectionStatus()
    ui.toast('飞书设置已保存', draft.value.enabled ? '连接器已启用。' : '连接器已暂停。')
  } catch (error) {
    ui.toast('飞书设置保存失败', apiErrorMessage(error, '请刷新后重试。'), 'danger')
  } finally {
    saving.value = false
  }
}

function clearCredentialsChanged(): void {
  if (!draft.value?.clearCredentials) return
  draft.value.appId = ''
  draft.value.appSecret = ''
  draft.value.enabled = false
}

async function testConnection(): Promise<void> {
  if (testing.value) return
  testing.value = true
  try {
    connectionTest.value = await adminApi.testFeishuConnection()
    await loadSettings()
    connection.value = await adminApi.connectionStatus()
    ui.toast(
      connectionTest.value.runnable ? '连接测试通过' : '连接测试未通过',
      connectionTest.value.runnable ? '所有已启用来源、字段映射和存储均可用。' : '请查看表级错误。',
      connectionTest.value.runnable ? 'success' : 'danger',
    )
  } catch (error) {
    ui.toast('连接测试失败', apiErrorMessage(error, '请检查已保存的飞书凭据、来源和存储。'), 'danger')
  } finally {
    testing.value = false
  }
}

async function startSync(): Promise<void> {
  if (starting.value) return
  starting.value = true
  try {
    const run = await adminApi.startFeishuSync()
    await runPager.refresh()
    await selectRun(run)
    ui.toast('全量同步已进入队列', '系统将完整扫描所有已启用来源表。')
  } catch (error) {
    ui.toast('无法启动同步', apiErrorMessage(error, '请先启用连接器并通过连接测试。'), 'danger')
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

onMounted(refreshPage)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="FEISHU PRODUCT MASTER" title="飞书单向同步" description="选择需要同步的飞书表；事实字段和源附件经自动校验后直接发布，网站不会回写飞书或生成导出文件。">
      <template #actions><button class="button button--secondary" type="button" @click="refreshPage"><RefreshCw :size="16" />刷新</button></template>
    </PageHeader>

    <DataStatePanel v-if="pageError" state="error" :title="pageError" @retry="refreshPage" />

    <section class="sync-status-grid">
      <article><div class="sync-status-grid__icon"><Cable :size="20" /></div><span>连接状态</span><strong>{{ connection?.runnable ? '可运行' : '未就绪' }}</strong><small>{{ connection?.unavailableReason || connection?.displayName || '等待连接测试' }}</small></article>
      <article><div class="sync-status-grid__icon"><Clock3 :size="20" /></div><span>最近运行</span><strong>{{ formatDate(latestRun?.startedAt) }}</strong><small>{{ latestRun?.status || '等待首次全量同步' }}</small></article>
      <article><div class="sync-status-grid__icon"><Database :size="20" /></div><span>产品</span><strong>{{ latestRun?.recordsApplied ?? 0 }} 应用 / {{ latestRun?.recordsDeleted ?? 0 }} 删除</strong><small>失败 {{ latestRun?.recordsFailed ?? 0 }} 条</small></article>
      <article><div class="sync-status-grid__icon"><Database :size="20" /></div><span>附件</span><strong>{{ latestRun?.assetsCopied ?? 0 }} 新增 / {{ latestRun?.assetsReused ?? 0 }} 复用</strong><small>失败 {{ latestRun?.assetsFailed ?? 0 }} 个</small></article>
    </section>

    <section class="panel settings-panel">
      <header><div><p class="eyebrow">CONNECTION & SCHEDULE</p><h2>连接、来源与调度</h2></div><StatusBadge :label="draft?.enabled ? '已启用' : '已暂停'" :tone="draft?.enabled ? 'success' : 'neutral'" /></header>
      <fieldset v-if="draft" class="settings-fieldset" :disabled="!canManageSettings">
        <div class="settings-grid">
          <div class="credential-status"><KeyRound :size="18" /><span><strong>App Secret {{ settings?.secretConfigured ? '已配置' : '尚未配置' }}</strong><small>Secret 保存于 PostgreSQL；读取 API 永不回显，留空会保留当前值。</small></span></div>
          <label class="field"><span>App ID</span><input v-model.trim="draft.appId" type="text" autocomplete="off" :disabled="draft.clearCredentials" /></label>
          <label class="field"><span>App Secret</span><input v-model="draft.appSecret" type="password" autocomplete="new-password" placeholder="留空保留已保存的 Secret" :disabled="draft.clearCredentials" /></label>
          <label class="field field--check"><input v-model="draft.clearCredentials" type="checkbox" @change="clearCredentialsChanged" /><span>清除已保存的飞书凭据</span></label>
          <label class="field field--check"><input v-model="draft.enabled" type="checkbox" /><span>启用飞书连接器</span></label>
          <label class="field field--check"><input v-model="draft.intervalEnabled" type="checkbox" /><span>启用间隔同步</span></label>
          <label class="field"><span>间隔（5–1440 分钟）</span><input v-model.number="draft.intervalMinutes" type="number" min="5" max="1440" :disabled="!draft.intervalEnabled" /></label>
          <label class="field field--check"><input v-model="draft.dailyEnabled" type="checkbox" /><span>启用每日同步</span></label>
          <label class="field"><span>每日时间（Asia/Shanghai）</span><input v-model="draft.dailyLocalTime" type="time" :disabled="!draft.dailyEnabled" /></label>
        </div>
        <div class="source-settings">
          <header><div><p class="eyebrow">SELECTED SOURCES</p><h3>选择性同步来源表</h3></div><button class="button button--secondary" type="button" @click="addSource"><Plus :size="16" />添加来源</button></header>
          <p v-if="!draft.sources.length" class="muted">尚未配置来源。连接器启用前至少启用一张表。</p>
          <div class="source-grid">
            <article v-for="(source, index) in draft.sources" :key="index" class="source-card">
              <header><label class="field field--check"><input v-model="source.enabled" type="checkbox" /><strong>来源 {{ index + 1 }}</strong></label><button class="icon-button" type="button" :aria-label="`移除来源 ${index + 1}`" @click="removeSource(index)"><Trash2 :size="16" /></button></header>
              <label class="field"><span>显示名称</span><input v-model.trim="source.name" type="text" /></label>
              <label class="field"><span>Wiki Token</span><input v-model.trim="source.wikiToken" type="text" autocomplete="off" /></label>
              <label class="field"><span>Table ID</span><input v-model.trim="source.tableId" type="text" autocomplete="off" /></label>
              <label class="field"><span>产品族</span><select v-model="source.family"><option v-for="family in familyOptions" :key="family.value" :value="family.value">{{ family.label }}</option></select></label>
              <label class="field"><span>应用分类（可选）</span><input v-model="source.application" type="text" placeholder="例如 agriculture-livestock" /></label>
            </article>
          </div>
          <small>停用或移除来源会立即隐藏其产品，并触发后台物理清理；若需恢复，请先在飞书恢复来源再重新同步。</small>
        </div>
      </fieldset>
      <p v-if="!canManageSettings" class="muted">当前账号可查看和运行同步，但需要 settings.manage 权限才能修改设置。</p>
      <div class="button-row"><button v-if="canManageSettings" class="button button--primary" type="button" :disabled="!draft || saving" @click="saveSettings"><Save :size="16" />{{ saving ? '保存中…' : '保存设置' }}</button><button class="button button--secondary" type="button" :disabled="testing || !settings?.secretConfigured" @click="testConnection"><Cable :size="16" />{{ testing ? '测试中…' : '测试已保存配置' }}</button><StatusBadge :label="configurationTested ? '当前配置已验证' : '当前配置未验证'" :tone="configurationTested ? 'success' : 'warning'" /></div>
      <div v-if="connectionTest" class="connection-result"><p>配置 revision {{ connectionTest.connectionRevision }} · 对象存储 {{ connectionTest.objectStorageReady ? '就绪' : '失败' }} · 私有暂存 {{ connectionTest.privateStagingReady ? '就绪' : '失败' }}</p><div class="data-table-wrap"><table class="data-table"><thead><tr><th>数据表</th><th>访问</th><th>字段</th><th>Mapping</th><th>错误</th></tr></thead><tbody><tr v-for="table in connectionTest.tables" :key="`${table.wikiToken}:${table.tableId}`"><td><strong>{{ table.name }}</strong><br><code>{{ table.wikiToken }} / {{ table.tableId }}</code></td><td><StatusBadge :label="table.accessible ? '可读取' : '失败'" :tone="table.accessible ? 'success' : 'danger'" /></td><td>{{ table.fieldCount }}</td><td><StatusBadge :label="table.mappingValid ? '有效' : '无效'" :tone="table.mappingValid ? 'success' : 'danger'" /></td><td>{{ table.errors.join('；') || '—' }}</td></tr></tbody></table></div></div>
    </section>

    <section class="panel sync-panel">
      <header class="run-header"><div><p class="eyebrow">FULL-SCAN RUNS</p><h2>逐表运行日志</h2></div><button class="button button--primary" type="button" :disabled="starting || !connection?.runnable" @click="startSync"><Play :size="16" />{{ starting ? '正在入队…' : '立即完整同步' }}</button></header>
      <DataStatePanel v-if="runsState !== 'ready'" :state="runsState" :title="runsState === 'empty' ? '尚无同步运行' : runPager.error.value || ''" @retry="runPager.refresh" />
      <div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>开始时间</th><th>触发</th><th>状态</th><th>读取 / 应用 / 失败 / 删除</th><th>附件</th><th></th></tr></thead><tbody><tr v-for="run in runs" :key="run.id"><td>{{ formatDate(run.startedAt) }}<br><code>{{ run.id }}</code></td><td>{{ run.trigger }}</td><td><StatusBadge :label="run.status" :tone="runTone(run.status)" /></td><td>{{ run.recordsSeen }} / {{ run.recordsApplied }} / {{ run.recordsFailed }} / {{ run.recordsDeleted }}</td><td>{{ run.assetsCopied }} 新增，{{ run.assetsReused }} 复用</td><td><button class="button button--secondary" type="button" :disabled="loadingDetail" @click="selectRun(run)">详情</button></td></tr></tbody></table></div>
      <CursorPaginationControls v-if="runsState === 'ready'" :item-count="runs.length" :page-number="runPager.pageNumber.value" :can-previous="runPager.canPrevious.value" :can-next="runPager.canNext.value" :loading="runPager.loading.value" label="条同步运行" @previous="runPager.previous" @next="runPager.next" />
      <div v-if="selectedDetail" class="run-detail"><header><div><p class="eyebrow">FROZEN CONFIG REVISION {{ selectedDetail.run.settingsRevision }}</p><h3>{{ selectedDetail.run.id }}</h3></div><span>{{ selectedDetail.run.sources.length }} 张来源表</span></header><div class="data-table-wrap"><table class="data-table"><thead><tr><th>来源表</th><th>状态</th><th>接收 / 应用 / 失败 / 删除</th><th>附件：发现 / 新增 / 复用 / 失败</th><th>错误</th></tr></thead><tbody><tr v-for="table in selectedDetail.tables" :key="`${table.wikiToken}:${table.tableId}`"><td><strong>{{ table.sourceName }}</strong><br><code>{{ table.wikiToken }} / {{ table.tableId }}</code></td><td><StatusBadge :label="table.status" :tone="table.status === 'completed' ? 'success' : table.status === 'failed' ? 'danger' : 'info'" /></td><td>{{ table.recordsSeen }} / {{ table.recordsApplied }} / {{ table.recordsFailed }} / {{ table.recordsDeleted }}</td><td>{{ table.assetsSeen }} / {{ table.assetsCopied }} / {{ table.assetsReused }} / {{ table.assetsFailed }}</td><td>{{ table.error || '—' }}</td></tr></tbody></table></div><p v-if="!selectedDetail.errors.length" class="muted">该运行没有校验、附件或表级错误。</p><div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>级别</th><th>记录</th><th>错误代码</th><th>字段</th><th>详情</th><th>时间</th></tr></thead><tbody><tr v-for="error in selectedDetail.errors" :key="`${error.createdAt}-${error.code}-${error.sourceRecordId}`"><td><StatusBadge :label="error.severity" :tone="error.severity === 'error' ? 'danger' : 'warning'" /></td><td><code>{{ error.sourceRecordId || '表级' }}</code></td><td><span class="error-code"><AlertTriangle :size="14" />{{ error.code }}</span></td><td>{{ error.fieldPath || '—' }}</td><td>{{ error.message }}</td><td>{{ formatDate(error.createdAt) }}</td></tr></tbody></table></div></div>
    </section>
  </div>
</template>

<style scoped>
@layer components {
.settings-panel,.sync-panel,.settings-fieldset,.source-settings,.connection-result { display: grid; gap: 1rem; }
.settings-fieldset { margin: 0; padding: 0; border: 0; min-inline-size: 0; }
.settings-panel > header,.run-header,.run-detail > header,.source-settings > header,.source-card > header { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.settings-grid { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 1rem; }
.credential-status { display: flex; align-items: center; gap: .6rem; grid-column: 1 / -1; padding: .75rem; border: 1px solid var(--color-border); border-radius: .65rem; }
.credential-status span { display: grid; gap: .15rem; }
.credential-status small,.source-settings small,.muted { color: var(--color-text-muted); }
.source-grid { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: .75rem; }
.source-card { display: grid; gap: .65rem; padding: .9rem; border: 1px solid var(--color-border); border-radius: .65rem; }
.field--check { display: flex; flex-direction: row; align-items: center; gap: .6rem; }
.field--check input { width: auto; }
.button-row { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
.run-detail { border-top: 1px solid var(--color-border); padding-top: 1rem; display: grid; gap: .8rem; }
.run-detail h3 { font-family: monospace; font-size: .9rem; }
.error-code { display: inline-flex; align-items: center; gap: .3rem; color: var(--color-danger); }
@media (max-width: 760px) { .settings-grid,.source-grid { grid-template-columns: 1fr; } .settings-panel > header,.run-header,.run-detail > header,.source-settings > header { align-items: flex-start; flex-direction: column; } }
}
</style>
