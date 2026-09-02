<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { AlertTriangle, CheckCircle2, Clock3, Database, HardDriveDownload, History, Play, RefreshCw, SearchCheck, ShieldAlert, X } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendOperation } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const selectedTask = ref<string | null>(null)
const confirmation = ref('')
const otp = ref('')
const operationPager = useCursorPagination<BackendOperation>(adminApi.listOperations, {
  errorMessage: '请检查 API 会话。',
  onError: (message) => ui.toast('任务记录读取失败', message, 'danger'),
})
const operationRuns = computed(() => operationPager.items.value)
const runsState = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (operationPager.loading.value && !operationRuns.value.length) return 'loading'
  if (operationPager.errorStatus.value === 403) return 'forbidden'
  if (operationPager.error.value) return 'error'
  return operationRuns.value.length ? 'ready' : 'empty'
})

const tasks = [
  { id: 'migrationPreflight', name: 'Migration 预检', detail: '检查待执行的预编译 migration 与数据库兼容性', icon: Database, risk: 'medium', confirmation: 'PREFLIGHT MIGRATION' },
  { id: 'backup', name: '创建加密备份', detail: '创建校验和、写入私有对象存储并验证可读性', icon: HardDriveDownload, risk: 'medium', confirmation: 'CREATE BACKUP' },
  { id: 'restoreValidate', name: '隔离恢复演练', detail: '恢复到独立目标，验证后生成受控切换方案', icon: RefreshCw, risk: 'high', confirmation: 'VALIDATE RESTORE' },
  { id: 'retentionApply', name: '执行保留期清理', detail: '处理已过宽限期的 PII，保留匿名聚合与审计', icon: History, risk: 'high', confirmation: 'APPLY RETENTION' },
  { id: 'searchReindex', name: '重建搜索索引', detail: '从 published projection 重建 FTS 与 trigram 索引', icon: SearchCheck, risk: 'low', confirmation: 'REBUILD SEARCH INDEX' },
]

const operation = computed(() => tasks.find((task) => task.id === selectedTask.value))
const ready = computed(() => Boolean(operation.value) && confirmation.value === operation.value?.confirmation && /^\d{6,8}$/.test(otp.value))

function openTask(id: string): void {
  selectedTask.value = id
  confirmation.value = ''
  otp.value = ''
}

async function runTask(): Promise<void> {
  if (!ready.value) return
  try {
    const run = await adminApi.createOperation(operation.value!.id, `${operation.value!.name} requested through the controlled Admin Web workflow.`, operation.value!.confirmation, otp.value)
    operationPager.items.value = [run, ...operationPager.items.value]
    ui.toast('任务已加入队列', `Operation ${run.id}`, 'info')
    selectedTask.value = null
  } catch (error) {
    ui.toast('任务创建失败', error instanceof Error ? error.message : '请检查权限与 TOTP。', 'danger')
  }
}

onMounted(async () => {
  await operationPager.first()
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="SAFE OPERATIONS" title="运维任务" description="仅运行版本化、预定义任务；生产后台不提供 SQL 控制台或任意数据库 Shell。" />

    <div class="operations-warning"><ShieldAlert :size="19" /><div><strong>安全执行模型</strong><p>高风险操作必须具备 Super Admin 权限、重新验证 TOTP，并输入任务目标确认文本。</p></div></div>

    <section class="operations-grid">
      <article v-for="task in tasks" :key="task.id" class="panel operation-card">
        <header><span :class="`risk-${task.risk}`"><component :is="task.icon" :size="20" /></span><StatusBadge :label="task.risk === 'high' ? '高风险' : task.risk === 'medium' ? '需确认' : '低风险'" :tone="task.risk === 'high' ? 'danger' : task.risk === 'medium' ? 'warning' : 'neutral'" /></header>
        <h2>{{ task.name }}</h2><p>{{ task.detail }}</p>
        <footer><small>Predefined operation · version 1</small><button type="button" @click="openTask(task.id)"><Play :size="15" />运行</button></footer>
      </article>
    </section>

    <section class="panel table-panel">
      <header class="panel__header"><div><p class="eyebrow">RECENT OPERATIONS</p><h2>运行记录</h2></div><StatusBadge label="平台任务" tone="info" /></header>
      <DataStatePanel
        v-if="runsState !== 'ready'"
        :state="runsState"
        :title="runsState === 'empty' ? '暂无运维任务记录' : runsState === 'error' ? operationPager.error.value || '' : ''"
        @retry="operationPager.refresh"
      />
      <div v-else class="data-table-wrap"><table class="data-table"><thead><tr><th>Operation ID</th><th>任务</th><th>状态</th><th>来源</th><th>开始时间</th><th>结果</th></tr></thead><tbody><tr v-for="run in operationRuns" :key="run.id"><td><code>{{ run.id }}</code></td><td>{{ run.kind }}</td><td><StatusBadge :label="run.status" :tone="run.status === 'completed' ? 'success' : run.status === 'failed' ? 'danger' : 'info'" /></td><td>Admin API</td><td>{{ new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(run.createdAt)) }}</td><td><CheckCircle2 v-if="run.status === 'completed'" :size="15" /><Clock3 v-else :size="15" />{{ run.result ? '有结果' : '等待 Worker' }}</td></tr></tbody></table></div>
      <CursorPaginationControls
        v-if="runsState === 'ready'"
        :item-count="operationRuns.length"
        :page-number="operationPager.pageNumber.value"
        :can-previous="operationPager.canPrevious.value"
        :can-next="operationPager.canNext.value"
        :loading="operationPager.loading.value"
        label="条运行记录"
        @previous="operationPager.previous"
        @next="operationPager.next"
      />
    </section>

    <div v-if="operation" class="modal-overlay" @click.self="selectedTask = null">
      <form class="modal-card" @submit.prevent="runTask">
        <header><div><p class="eyebrow">CONFIRM OPERATION</p><h2>{{ operation.name }}</h2></div><button type="button" class="icon-button" aria-label="关闭" @click="selectedTask = null"><X :size="18" /></button></header>
        <div class="modal-warning"><AlertTriangle :size="18" /><p>{{ operation.detail }}。正式环境会先执行预检，并将输出写入不可变任务日志。</p></div>
        <label class="field"><span>输入确认文本</span><code class="confirmation-code">{{ operation.confirmation }}</code><input v-model="confirmation" autocomplete="off" /></label>
        <label class="field"><span>TOTP 验证码</span><input v-model="otp" maxlength="8" inputmode="numeric" autocomplete="one-time-code" placeholder="••••••" /></label>
        <footer><button class="button button--quiet" type="button" @click="selectedTask = null">取消</button><button class="button button--primary" type="submit" :disabled="!ready">确认并加入队列</button></footer>
      </form>
    </div>
  </div>
</template>
