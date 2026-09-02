<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { CheckCircle2, FileSpreadsheet, LockKeyhole, RefreshCw, UploadCloud } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import PermissionGate from '@/components/PermissionGate.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, productImportResult, waitForOperation, type ProductImportResult } from '@/services/adminApi'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const file = ref<File>()
const importing = ref(false)
const result = ref<ProductImportResult>()
const importPager = useCursorPagination<ProductImportResult>((pagination) => adminApi.listProductImports(pagination), {
  pageSize: 20,
  errorMessage: '导入历史读取失败。',
  onError: (message) => ui.toast('导入历史读取失败', message, 'danger'),
})
const history = computed(() => importPager.items.value)
const historyState = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (importPager.loading.value && !history.value.length) return 'loading'
  if (importPager.errorStatus.value === 403) return 'forbidden'
  if (importPager.error.value) return 'error'
  return history.value.length ? 'ready' : 'empty'
})

function selectFile(event: Event): void {
  const input = event.target as HTMLInputElement
  file.value = input.files?.[0]
  result.value = undefined
}

async function startImport(): Promise<void> {
  if (!file.value) return
  if (file.value.size > 16 * 1024 * 1024) {
    ui.toast('文件过大', 'Product Master CSV 上限为 16 MiB。', 'warning')
    return
  }
  importing.value = true
  try {
    // Product Master authority is bound to the exact source bytes. Preserve a
    // UTF-8 BOM instead of letting Blob.text() silently remove it before the
    // Rust service computes the registered SHA-256.
    const csv = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(
      await file.value.arrayBuffer(),
    )
    const accepted = await adminApi.importProductMaster(csv)
    ui.toast('Product Master 已接收', `Operation ${accepted.operationId.slice(0, 8)} 状态：${accepted.status}。`, 'info')
    const operation = await waitForOperation(accepted)
    result.value = productImportResult(operation)
    ui.toast('Product Master 导入完成', `${result.value.validRows} 条有效记录，${result.value.malformedRows} 条残缺记录。`)
    await importPager.first()
  } catch (error) {
    ui.toast('导入失败', apiErrorMessage(error, '请检查 CSV、密钥和权限。'), 'danger')
  } finally {
    importing.value = false
  }
}

onMounted(importPager.first)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="VERIFIED PRODUCT MASTER" title="产品数据导入" description="CSV 先写入可审计 Staging，再生成不可变产品 revision；HTML 原型数据永不参与导入。">
      <template #actions><button class="button button--secondary" type="button" :disabled="importPager.loading.value" @click="importPager.refresh"><RefreshCw :size="16" />刷新历史</button></template>
    </PageHeader>

    <section class="panel import-dropzone">
      <UploadCloud :size="34" />
      <div><h2>选择已验证 Product Master CSV</h2><p>仅接受 UTF-8 CSV。重复文件由 SHA-256 与 mapping version 幂等识别。</p></div>
      <label class="button button--secondary"><FileSpreadsheet :size="16" />{{ file?.name ?? '选择 CSV' }}<input class="sr-only" type="file" accept=".csv,text/csv" @change="selectFile" /></label>
      <button class="button button--primary" type="button" :disabled="!file || importing" @click="startImport">{{ importing ? '正在校验并导入…' : '开始导入' }}</button>
    </section>

    <div class="security-baseline"><LockKeyhole :size="18" /><div><strong>报价只存入加密私有 Staging</strong><p>币种不做推断，报价不进入产品 revision、公开 API、SSR、日志或导出。</p></div><PermissionGate permission="product.pricing.read"><StatusBadge label="可查看私有字段" tone="warning" /><template #fallback><StatusBadge label="私有字段已隐藏" tone="success" /></template></PermissionGate></div>

    <section v-if="result" class="panel import-result">
      <header class="panel__header"><div><p class="eyebrow">LATEST IMPORT</p><h2>导入报告</h2></div><StatusBadge :label="result.reused ? '幂等复用' : result.status" :tone="result.status === 'failed' ? 'danger' : 'success'" /></header>
      <div class="import-metrics"><div><strong>{{ result.validRows }}</strong><span>有效产品</span></div><div><strong>{{ result.malformedRows }}</strong><span>残缺行</span></div><div><strong>{{ result.missingAssets.length }}</strong><span>缺失附件</span></div><div><strong>{{ result.mappingVersion }}</strong><span>Mapping</span></div></div>
      <p class="checksum"><CheckCircle2 :size="15" />SHA-256 <code>{{ result.checksum }}</code></p>
      <div v-if="result.errors.length" class="data-table-wrap"><table class="data-table"><thead><tr><th>级别</th><th>行</th><th>型号</th><th>字段</th><th>代码</th><th>说明</th></tr></thead><tbody><tr v-for="error in result.errors" :key="`${error.rowNumber}-${error.code}-${error.fieldName}`"><td><StatusBadge :label="error.severity" :tone="error.severity === 'error' ? 'danger' : 'warning'" /></td><td>{{ error.rowNumber }}</td><td>{{ error.stableId || '—' }}</td><td>{{ error.fieldName || '整行' }}</td><td><code>{{ error.code }}</code></td><td>{{ error.detail }}</td></tr></tbody></table></div>
    </section>

    <DataStatePanel v-if="historyState !== 'ready'" :state="historyState" :title="historyState === 'empty' ? '暂无导入历史' : ''" @retry="importPager.refresh" />
    <section v-else class="panel table-panel"><header class="panel__header table-panel__header"><div><p class="eyebrow">AUDIT TRAIL</p><h2>导入历史</h2></div></header><div class="data-table-wrap"><table class="data-table"><thead><tr><th>时间</th><th>Checksum</th><th>Mapping</th><th>有效/总行</th><th>缺失附件</th><th>状态</th></tr></thead><tbody><tr v-for="item in history" :key="item.id"><td>{{ new Date(item.createdAt).toLocaleString('zh-CN') }}</td><td><code>{{ item.checksum.slice(0, 12) }}…</code></td><td>{{ item.mappingVersion }}</td><td>{{ item.validRows }} / {{ item.totalRows }}</td><td>{{ item.missingAssets.length }}</td><td><StatusBadge :label="item.reused ? 'reused' : item.status" :tone="item.status === 'failed' ? 'danger' : 'success'" /></td></tr></tbody></table></div><CursorPaginationControls :item-count="history.length" :page-number="importPager.pageNumber.value" :can-previous="importPager.canPrevious.value" :can-next="importPager.canNext.value" :loading="importPager.loading.value" label="条导入记录" @previous="importPager.previous" @next="importPager.next" /></section>
  </div>
</template>
