<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { CheckCircle2, RefreshCcw, Search, ShieldAlert, UploadCloud, X } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import {
  contentApi,
  type MediaAccessLevel,
  type MediaAssetSummary,
  type MediaScanStatus,
} from '@/services/contentApi'
import { reviewMediaAsset, uploadMediaAsset } from '@/services/mediaApi'
import { DEFAULT_ADMIN_PAGE_SIZE } from '@/services/cursorPagination'

type PageState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'
type ReviewDecision = 'clean' | 'quarantined'

const ACCEPTED_TYPES = 'image/png,image/jpeg,image/webp'
const MAX_UPLOAD_BYTES = 25 * 1024 * 1024
const items = ref<MediaAssetSummary[]>([])
const pageState = ref<PageState>('loading')
const errorMessage = ref('')
const actionMessage = ref('')
const query = ref('')
const reviewFilter = ref<MediaScanStatus | ''>('')
const accessFilter = ref<MediaAccessLevel | ''>('')
const uploading = ref(false)
const busyAssetId = ref('')
const uploadInput = ref<HTMLInputElement | null>(null)
const cursorStack = ref<string[]>([])
const nextCursor = ref<string | null>(null)
const reviewTarget = ref<MediaAssetSummary | null>(null)
const reviewDecision = ref<ReviewDecision>('clean')
const reviewReason = ref('')
const reviewError = ref('')
let searchTimer: ReturnType<typeof setTimeout> | undefined

const hasFilters = computed(() => Boolean(query.value.trim() || reviewFilter.value || accessFilter.value))
const reviewTitle = computed(() => {
  const name = reviewTarget.value?.originalName ?? '媒体'
  return reviewDecision.value === 'clean' ? `标记 ${name} 为可用` : `隔离 ${name}`
})
const reviewButton = computed(() => reviewDecision.value === 'clean' ? '确认可用' : '确认隔离')

function reviewTone(status: MediaScanStatus): 'success' | 'warning' | 'danger' | 'neutral' {
  if (status === 'clean') return 'success'
  if (status === 'pending') return 'warning'
  if (status === 'quarantined' || status === 'failed') return 'danger'
  return 'neutral'
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / (1024 * 1024)).toFixed(1)} MB`
}

async function load(reset = false): Promise<void> {
  if (reset) {
    cursorStack.value = []
    nextCursor.value = null
  }
  pageState.value = 'loading'
  errorMessage.value = ''
  try {
    const page = await contentApi.listMediaAssets({
      q: query.value.trim() || undefined,
      scanStatus: reviewFilter.value || undefined,
      accessLevel: accessFilter.value || undefined,
      cursor: cursorStack.value.at(-1),
      limit: DEFAULT_ADMIN_PAGE_SIZE,
    })
    items.value = page.items
    nextCursor.value = page.nextCursor
    pageState.value = page.items.length ? 'ready' : 'empty'
  }
  catch (error) {
    if (error instanceof ApiError && error.status === 403) {
      pageState.value = 'forbidden'
      return
    }
    errorMessage.value = error instanceof Error ? error.message : '媒体库读取失败。'
    pageState.value = 'error'
  }
}

function nextPage(): void {
  if (!nextCursor.value) return
  cursorStack.value = [...cursorStack.value, nextCursor.value]
  void load()
}

function previousPage(): void {
  if (!cursorStack.value.length) return
  cursorStack.value = cursorStack.value.slice(0, -1)
  void load()
}

function scheduleSearch(): void {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => void load(true), 250)
}

async function handleFileSelection(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  if (file.size > MAX_UPLOAD_BYTES) {
    actionMessage.value = '文件超过 25 MiB 上限，请压缩后重试。'
    return
  }
  uploading.value = true
  try {
    await uploadMediaAsset(file)
    actionMessage.value = `${file.name} 已上传，等待人工审核。`
    await load(true)
  }
  catch (error) {
    actionMessage.value = error instanceof Error ? error.message : '上传失败。'
  }
  finally {
    uploading.value = false
  }
}

function openReview(asset: MediaAssetSummary, decision: ReviewDecision): void {
  reviewTarget.value = asset
  reviewDecision.value = decision
  reviewReason.value = ''
  reviewError.value = ''
}

function closeReview(): void {
  reviewTarget.value = null
  reviewReason.value = ''
  reviewError.value = ''
}

async function confirmReview(): Promise<void> {
  const asset = reviewTarget.value
  if (!asset) return
  if (!reviewReason.value.trim()) {
    reviewError.value = '人工审核决定必须填写原因。'
    return
  }
  busyAssetId.value = asset.id
  try {
    await reviewMediaAsset(asset.id, reviewDecision.value, reviewReason.value)
    actionMessage.value = `${reviewTitle.value} 已完成。`
    closeReview()
    await load()
  }
  catch (error) {
    reviewError.value = error instanceof Error ? error.message : '人工审核状态更新失败。'
  }
  finally {
    busyAssetId.value = ''
  }
}

onMounted(() => void load(true))
onBeforeUnmount(() => {
  if (searchTimer) clearTimeout(searchTimer)
})
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="ASSET LIBRARY" title="媒体中心" description="上传资源默认保持待审核；只有带原因的人工决定才能标记为可用或隔离。">
      <template #actions>
        <input ref="uploadInput" class="sr-only" type="file" :accept="ACCEPTED_TYPES" aria-label="选择要上传的图片" @change="handleFileSelection">
        <button class="button button--primary" type="button" :disabled="uploading" @click="uploadInput?.click()"><UploadCloud :size="16" />{{ uploading ? '正在上传' : '上传资源' }}</button>
      </template>
    </PageHeader>

    <section class="media-toolbar panel" aria-label="媒体筛选">
      <label class="media-toolbar__search"><Search :size="15" aria-hidden="true" /><span class="sr-only">按文件名搜索</span><input v-model="query" type="search" placeholder="按文件名搜索" @input="scheduleSearch"></label>
      <label><span>人工审核</span><select v-model="reviewFilter" @change="load(true)"><option value="">全部</option><option value="pending">待审核</option><option value="clean">可用</option><option value="quarantined">已隔离</option><option value="failed">失败</option></select></label>
      <label><span>访问</span><select v-model="accessFilter" @change="load(true)"><option value="">全部</option><option value="public">公开</option><option value="authenticated">需认证</option><option value="internal">内部</option></select></label>
      <button class="button button--quiet" type="button" @click="load(true)"><RefreshCcw :size="15" />刷新</button>
    </section>

    <p v-if="actionMessage" class="media-notice" role="status">{{ actionMessage }}</p>
    <DataStatePanel
      v-if="pageState !== 'ready'"
      :state="pageState === 'loading' ? 'loading' : pageState === 'forbidden' ? 'forbidden' : pageState === 'error' ? 'error' : 'empty'"
      :title="pageState === 'error' ? errorMessage : pageState === 'empty' ? (hasFilters ? '没有匹配筛选条件的媒体' : '媒体库为空') : ''"
      :description="pageState === 'empty' && !hasFilters ? '上传 PNG、JPEG 或 WebP 图片后在此完成人工审核。' : ''"
      @retry="load(true)"
    />

    <section v-else class="panel table-panel">
      <div class="data-table-wrap"><table class="data-table">
        <caption class="sr-only">媒体资源列表，本页 {{ items.length }} 条</caption>
        <thead><tr><th>资源</th><th>人工审核</th><th>访问</th><th>操作</th></tr></thead>
        <tbody><tr v-for="asset in items" :key="asset.id">
          <td><strong>{{ asset.originalName }}</strong><span>{{ asset.mediaType }} · {{ formatBytes(asset.byteSize) }}</span><span>版本 {{ asset.versionId.slice(0, 8) }}…</span></td>
          <td><StatusBadge :label="asset.scanStatus" :tone="reviewTone(asset.scanStatus)" /></td>
          <td>{{ asset.accessLevel }}</td>
          <td class="media-actions">
            <button class="button button--quiet" type="button" :disabled="busyAssetId === asset.id || asset.scanStatus === 'clean'" @click="openReview(asset, 'clean')"><CheckCircle2 :size="14" />标记可用</button>
            <button class="button button--quiet" type="button" :disabled="busyAssetId === asset.id || asset.scanStatus === 'quarantined'" @click="openReview(asset, 'quarantined')"><ShieldAlert :size="14" />隔离</button>
          </td>
        </tr></tbody>
      </table></div>
      <div class="media-pagination"><span>本页 {{ items.length }} 条</span><div><button class="button button--quiet" :disabled="!cursorStack.length" @click="previousPage">上一页</button><button class="button button--quiet" :disabled="!nextCursor" @click="nextPage">下一页</button></div></div>
    </section>

    <div v-if="reviewTarget" class="media-dialog" role="dialog" aria-modal="true" aria-labelledby="media-review-title"><div class="media-dialog__panel panel">
      <h2 id="media-review-title">{{ reviewTitle }}</h2><p>这是独立的人工审核决定；系统不会自动改变该状态。</p>
      <label><span>原因</span><textarea v-model="reviewReason" rows="3" maxlength="500" required aria-describedby="media-review-error" /></label>
      <p v-if="reviewError" id="media-review-error" class="media-dialog__error" role="alert">{{ reviewError }}</p>
      <div class="media-dialog__actions"><button class="button button--quiet" type="button" @click="closeReview"><X :size="15" />取消</button><button class="button button--primary" type="button" :disabled="busyAssetId === reviewTarget.id" @click="confirmReview">{{ reviewButton }}</button></div>
    </div></div>
  </div>
</template>

<style scoped>
.media-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: .75rem; }
.media-toolbar label { display: inline-flex; align-items: center; gap: .35rem; font-size: .85rem; }
.media-toolbar__search { flex: 1 1 16rem; }.media-toolbar__search input { flex: 1; }
.media-toolbar select, .media-toolbar input { padding: .35rem .5rem; }
.media-notice { margin: 0; padding: .5rem .75rem; border-radius: .5rem; background: var(--surface-muted, #f3f4f6); font-size: .85rem; }
.media-actions { display: flex; min-width: 12rem; gap: .3rem; flex-wrap: wrap; }
.media-pagination { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .6rem .25rem 0; font-size: .82rem; }.media-pagination div { display: flex; gap: .4rem; }
.media-dialog { position: fixed; inset: 0; display: grid; place-items: center; padding: 1rem; background: rgb(15 23 42 / 45%); z-index: 40; }
.media-dialog__panel { width: min(38rem, 92vw); display: grid; gap: .6rem; }.media-dialog__panel label { display: grid; gap: .3rem; font-size: .85rem; }
.media-dialog__error { margin: 0; color: var(--danger-text, #b91c1c); font-size: .85rem; }.media-dialog__actions { display: flex; justify-content: flex-end; gap: .5rem; }
</style>
