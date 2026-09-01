<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { AlertTriangle, ArrowRight, Braces, Cable, CheckCircle2, Clock3, Play, RefreshCw, Settings2, ShieldAlert, X } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, mockApiEnabled, type BackendSyncRun } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const activeTab = ref<'staging' | 'conflicts' | 'mapping' | 'runs'>('conflicts')
const showOverride = ref(false)
const overrideReason = ref('')
const overrideExpiry = ref('')
const dryRunActive = ref(false)
const demoConflicts = [
  { id: 'demo-conflict-1', sourceRecordId: 'DEMO-NOT-FOR-PUBLISH-001', detail: 'motorTechnology · 来源字段冲突' },
  { id: 'demo-conflict-2', sourceRecordId: 'DEMO-NOT-FOR-PUBLISH-002', detail: 'lifecycleStatus · 本地覆盖将到期' },
]
const demoDiffColumns = [
  { label: 'LAST ACCEPTED BASE', value: 'pendingVerification', detail: 'Accepted revision · demo', className: '' },
  { label: 'CURRENT LOCAL', value: 'EC', detail: '临时覆盖 · 剩余 11 天', className: 'diff-columns__local' },
  { label: 'INCOMING FEISHU', value: 'DC', detail: 'Staging snapshot · demo', className: 'diff-columns__incoming' },
]
const unavailableDiffColumns = [
  { label: 'LAST ACCEPTED BASE', value: '列表响应未提供', detail: '需要冲突详情端点', className: '' },
  { label: 'CURRENT LOCAL', value: '列表响应未提供', detail: '未推测本地字段值', className: 'diff-columns__local' },
  { label: 'INCOMING FEISHU', value: '列表响应未提供', detail: '未推测 Feishu 字段值', className: 'diff-columns__incoming' },
]

const syncRunPager = useCursorPagination<BackendSyncRun>(adminApi.listSyncRuns, {
  errorMessage: '无法读取同步运行记录。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const conflictPager = useCursorPagination<Record<string, unknown>>(adminApi.listConflicts, {
  errorMessage: '无法读取同步冲突。',
  onError: (message) => ui.toast('同步中心读取失败', message, 'danger'),
})
const syncRuns = computed(() => syncRunPager.items.value)
const conflicts = computed(() => conflictPager.items.value)
const visibleConflicts = computed(() => mockApiEnabled ? demoConflicts : conflicts.value)
const diffColumns = computed(() => mockApiEnabled ? demoDiffColumns : unavailableDiffColumns)

const tabs = computed(() => [
  { id: 'staging', label: 'Staging', count: mockApiEnabled ? 8 : 0 },
  { id: 'conflicts', label: '冲突', count: mockApiEnabled ? 2 : `${conflicts.value.length}${conflictPager.canNext.value ? '+' : ''}` },
  { id: 'mapping', label: '字段映射', count: 0 },
  { id: 'runs', label: '运行记录', count: syncRuns.value.length },
] as const)

const canCreateOverride = computed(() => overrideReason.value.trim().length >= 12 && Boolean(overrideExpiry.value))

async function runDryRun(): Promise<void> {
  dryRunActive.value = true
  try {
    if (mockApiEnabled) {
      await new Promise((resolve) => window.setTimeout(resolve, 800))
      ui.toast('Dry-run 已加入演示队列', '未连接真实 Feishu 数据。', 'info')
    } else {
      const run = await adminApi.startSync(true)
      syncRunPager.items.value = [run, ...syncRunPager.items.value]
      ui.toast('Dry-run 已加入队列', `Operation ${run.id}`, 'info')
    }
  } catch (error) {
    ui.toast('Dry-run 创建失败', error instanceof Error ? error.message : '请检查同步配置。', 'danger')
  } finally {
    dryRunActive.value = false
  }
}

function createOverride(): void {
  if (!canCreateOverride.value) return
  showOverride.value = false
  ui.toast('临时覆盖草稿已创建', '发布前仍需完成来源冲突校验。', 'warning')
}

onMounted(async () => {
  if (mockApiEnabled) return
  await Promise.all([syncRunPager.first(), conflictPager.first()])
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
      <article><div class="sync-status-grid__icon"><Cable :size="20" /></div><span>连接状态</span><strong>{{ mockApiEnabled ? '开发模拟连接' : '由后端配置管理' }}</strong><small>{{ mockApiEnabled ? '生产凭据未配置' : '浏览器不读取集成凭据' }}</small></article>
      <article><div class="sync-status-grid__icon"><Clock3 :size="20" /></div><span>最近同步</span><strong>{{ syncRuns[0] ? new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(syncRuns[0].startedAt)) : mockApiEnabled ? '今天 10:18' : '尚无记录' }}</strong><small>{{ syncRuns[0]?.status || (mockApiEnabled ? 'Dry-run · 演示记录' : '等待首次运行') }}</small></article>
      <article><div class="sync-status-grid__icon sync-status-grid__icon--warning"><ShieldAlert :size="20" /></div><span>阻塞冲突</span><strong>{{ mockApiEnabled ? 2 : `${conflicts.length}${conflictPager.canNext.value ? '+' : ''}` }}</strong><small>{{ conflictPager.canNext.value ? '当前页计数；未解决前不可再次发布' : '未解决前不可再次发布' }}</small></article>
      <article><div class="sync-status-grid__icon"><Braces :size="20" /></div><span>Mapping version</span><strong>v1 · Draft</strong><small>尚未用于生产同步</small></article>
    </section>

    <section class="panel sync-panel">
      <div class="tab-bar" role="tablist" aria-label="同步数据视图">
        <button v-for="tab in tabs" :key="tab.id" type="button" role="tab" :aria-selected="activeTab === tab.id" :class="{ 'is-active': activeTab === tab.id }" @click="activeTab = tab.id">
          {{ tab.label }}<span v-if="tab.count">{{ tab.count }}</span>
        </button>
      </div>

      <div v-if="activeTab === 'conflicts' && (mockApiEnabled || conflicts.length)" class="conflict-layout">
        <div class="conflict-list">
          <header><div><p class="eyebrow">BLOCKING</p><h2>字段冲突</h2></div><StatusBadge :label="`${mockApiEnabled ? 2 : conflicts.length} 个待处理`" tone="danger" /></header>
          <button v-for="(conflict, index) in visibleConflicts" :key="String(conflict.id || index)" type="button" class="conflict-item" :class="{ 'is-active': index === 0 }"><AlertTriangle :size="18" /><span><strong>{{ String(conflict.sourceRecordId || conflict.id || 'Unknown record') }}</strong><small>{{ String(conflict.detail || '三方差异 · 来源字段冲突') }}</small></span><ArrowRight :size="15" /></button>
          <CursorPaginationControls
            :item-count="mockApiEnabled ? 2 : conflicts.length"
            :page-number="conflictPager.pageNumber.value"
            :can-previous="!mockApiEnabled && conflictPager.canPrevious.value"
            :can-next="!mockApiEnabled && conflictPager.canNext.value"
            :loading="conflictPager.loading.value"
            label="条冲突记录"
            @previous="conflictPager.previous"
            @next="conflictPager.next"
          />
        </div>

        <div class="diff-view">
          <header><div><p class="eyebrow">THREE-WAY DIFF</p><h2>{{ mockApiEnabled ? 'motorTechnology' : 'Field-level diff' }}</h2><p>{{ mockApiEnabled ? '演示实体 · 此值不可用于公开产品' : '值由 Rust API 返回；处理前需确认字段所有权。' }}</p></div><button type="button" class="icon-button" aria-label="关闭差异"><X :size="17" /></button></header>
          <div class="diff-columns"><article v-for="column in diffColumns" :key="column.label" :class="column.className"><span>{{ column.label }}</span><code>{{ column.value }}</code><small>{{ column.detail }}</small></article></div>
          <div class="diff-warning"><ShieldAlert :size="18" /><div><strong>不要通过多数值推断事实</strong><p>当前值仅为冲突界面演示。必须由 Product Master 或受控工程记录解决。</p></div></div>
          <h3>处理方式</h3>
          <div class="resolution-grid">
            <button type="button"><CheckCircle2 :size="18" /><span><strong>接受 Feishu</strong><small>生成新的待发布 revision</small></span></button>
            <button type="button" @click="showOverride = true"><Clock3 :size="18" /><span><strong>创建临时覆盖</strong><small>必须填写原因与到期日</small></span></button>
          </div>
          <p class="diff-audit-note">所有处理都会记录 base、local、incoming、操作者、原因和时间。不会自动回写 Feishu。</p>
        </div>
      </div>

      <div v-else-if="activeTab === 'conflicts'" class="module-empty"><CheckCircle2 :size="27" /><h2>没有待处理冲突</h2><p>新同步仍会先进入 Staging；这里的空状态不代表 Feishu 已连接。</p></div>
      <div v-else-if="activeTab === 'staging'" class="module-empty"><Play :size="27" /><h2>{{ mockApiEnabled ? '8 条演示记录位于 Staging' : 'Staging 等待同步输入' }}</h2><p>执行校验后才会生成字段级 diff；Staging 永远不会直接成为公开数据。</p><button class="button button--primary" type="button" @click="runDryRun">运行校验</button></div>
      <div v-else-if="activeTab === 'mapping'" class="mapping-view">
        <header><div><p class="eyebrow">MAPPING V1 · DRAFT</p><h2>字段所有权与映射</h2></div><button class="button button--secondary" type="button">新建 mapping version</button></header>
        <table class="data-table"><thead><tr><th>Feishu 字段</th><th>平台字段</th><th>所有权</th><th>转换</th><th>校验</th></tr></thead><tbody><tr><td>产品唯一标识</td><td><code>source.externalId</code></td><td><StatusBadge label="Feishu" tone="info" /></td><td>Trim</td><td>唯一 / 必填</td></tr><tr><td>展示标题</td><td><code>presentation.title</code></td><td><StatusBadge label="Portal" tone="success" /></td><td>Locale map</td><td>英文必填</td></tr><tr><td>规格值</td><td><code>specs[].value</code></td><td><StatusBadge label="Feishu" tone="info" /></td><td>单位规范化</td><td>数值 / 工况 / 状态</td></tr></tbody></table>
      </div>
      <div v-else class="mapping-view sync-runs-view">
        <header><div><p class="eyebrow">SYNC RUNS</p><h2>同步运行记录</h2></div><button class="button button--secondary" type="button">查看 Worker 状态</button></header>
        <div v-if="syncRuns.length" class="data-table-wrap"><table class="data-table"><thead><tr><th>Run ID</th><th>模式</th><th>状态</th><th>已读取</th><th>通过校验</th><th>冲突</th><th>开始时间</th></tr></thead><tbody><tr v-for="run in syncRuns" :key="run.id"><td><code>{{ run.id }}</code></td><td>{{ run.dryRun ? 'Dry-run' : '正式同步' }}</td><td><StatusBadge :label="run.status" :tone="run.status === 'completed' ? 'success' : run.status === 'failed' ? 'danger' : 'info'" /></td><td>{{ run.recordsSeen }}</td><td>{{ run.recordsValid }}</td><td>{{ run.conflictCount }}</td><td>{{ new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(run.startedAt)) }}</td></tr></tbody></table></div>
        <div v-else class="module-empty module-empty--compact"><Clock3 :size="27" /><h2>尚无同步运行</h2><p>运行的 cursor、分页进度、重试和校验结果将保存在平台任务记录中。</p></div>
        <CursorPaginationControls
          :item-count="syncRuns.length"
          :page-number="syncRunPager.pageNumber.value"
          :can-previous="!mockApiEnabled && syncRunPager.canPrevious.value"
          :can-next="!mockApiEnabled && syncRunPager.canNext.value"
          :loading="syncRunPager.loading.value"
          label="条同步运行"
          @previous="syncRunPager.previous"
          @next="syncRunPager.next"
        />
      </div>
    </section>

    <div v-if="showOverride" class="modal-overlay" @click.self="showOverride = false">
      <form class="modal-card" @submit.prevent="createOverride">
        <header><div><p class="eyebrow">TEMPORARY OVERRIDE</p><h2>创建临时字段覆盖</h2></div><button type="button" class="icon-button" aria-label="关闭" @click="showOverride = false"><X :size="18" /></button></header>
        <div class="modal-warning"><AlertTriangle :size="18" /><p>覆盖仅影响网站发布 projection，不改变 Feishu 权威值，也不会静默续期。</p></div>
        <label class="field"><span>覆盖原因 <small>（至少 12 个字符）</small></span><textarea v-model="overrideReason" rows="4" required placeholder="说明工程依据、负责人和待完成的 Feishu 修正…"></textarea></label>
        <label class="field"><span>到期日</span><input v-model="overrideExpiry" type="date" required /></label>
        <footer><button class="button button--quiet" type="button" @click="showOverride = false">取消</button><button class="button button--primary" type="submit" :disabled="!canCreateOverride">创建覆盖草稿</button></footer>
      </form>
    </div>
  </div>
</template>
