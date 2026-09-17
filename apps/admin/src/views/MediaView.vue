<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { Copy, ExternalLink, FileImage, RefreshCcw, Search, UploadCloud, X } from 'lucide-vue-next'
import { ApiError, type MediaAsset, type MediaAssetReference } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { adminApi } from '@/services/adminApi'
import { contentApi } from '@/services/contentApi'
import { getMediaAsset, listMediaAssetReferences, uploadMediaAsset } from '@/services/mediaApi'
import { DEFAULT_ADMIN_PAGE_SIZE } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'

type PageState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'

const ACCEPTED_TYPES = 'image/png,image/jpeg,image/webp'
const MAX_UPLOAD_BYTES = 25 * 1024 * 1024
const apiOrigin = new URL(import.meta.env.VITE_ADMIN_API_BASE_URL ?? 'http://localhost:8080/api/admin/v1').origin
const auth = useAuthStore()
const route = useRoute()
const router = useRouter()
const items = ref<MediaAsset[]>([])
const total = ref(0)
const pageState = ref<PageState>('loading')
const errorMessage = ref('')
const notice = ref('')
const query = ref(typeof route.query.q === 'string' ? route.query.q : '')
const uploading = ref(false)
const storageConfigured = ref<boolean | null>(null)
const uploadInput = ref<HTMLInputElement | null>(null)
const cursorStack = ref<string[]>([])
const nextCursor = ref<string | null>(null)
const selected = ref<MediaAsset | null>(null)
const references = ref<MediaAssetReference[]>([])
const detailState = ref<'idle' | 'loading' | 'ready' | 'error'>('idle')
const detailError = ref('')
let searchTimer: ReturnType<typeof setTimeout> | undefined

const canWrite = computed(() => auth.hasPermission('media.write'))
const canManageStorage = computed(() => auth.hasPermission('settings.manage'))
const canUpload = computed(() => canWrite.value && storageConfigured.value !== false)
const hasFilters = computed(() => Boolean(query.value.trim()))

function absoluteMediaUrl(value: string): string {
  return new URL(value, apiOrigin).toString()
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

async function load(reset = false): Promise<void> {
  if (reset) cursorStack.value = []
  pageState.value = 'loading'
  errorMessage.value = ''
  try {
    const page = await contentApi.listMediaAssets({
      q: query.value.trim() || undefined,
      cursor: cursorStack.value.at(-1),
      limit: DEFAULT_ADMIN_PAGE_SIZE,
    })
    items.value = page.items
    total.value = page.total
    nextCursor.value = page.nextCursor
    pageState.value = page.items.length ? 'ready' : 'empty'
  }
  catch (error) {
    if (error instanceof ApiError && error.status === 403) pageState.value = 'forbidden'
    else {
      errorMessage.value = error instanceof Error ? error.message : '媒体库读取失败。'
      pageState.value = 'error'
    }
  }
}

async function loadStorageStatus(): Promise<void> {
  if (!canManageStorage.value) return
  try {
    storageConfigured.value = (await adminApi.getObjectStorageSettings()).settings.configured
  } catch {
    storageConfigured.value = null
  }
}

async function openDetail(asset: MediaAsset): Promise<void> {
  selected.value = asset
  references.value = []
  detailState.value = 'loading'
  detailError.value = ''
  try {
    const [detail, page] = await Promise.all([
      getMediaAsset(asset.id),
      listMediaAssetReferences(asset.id),
    ])
    selected.value = detail
    references.value = page.items
    detailState.value = 'ready'
  }
  catch (error) {
    detailError.value = error instanceof Error ? error.message : '媒体详情读取失败。'
    detailState.value = 'error'
  }
}

function closeDetail(): void {
  selected.value = null
  detailState.value = 'idle'
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
  searchTimer = setTimeout(() => {
    const q = query.value.trim()
    if ((typeof route.query.q === 'string' ? route.query.q : '') === q) {
      void load(true)
      return
    }
    void router.replace({ query: { ...route.query, ...(q ? { q } : { q: undefined }) } })
  }, 250)
}

async function handleFileSelection(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  if (file.size > MAX_UPLOAD_BYTES) {
    notice.value = '文件超过 25 MiB 上限。'
    return
  }
  uploading.value = true
  notice.value = ''
  try {
    const asset = await uploadMediaAsset(file)
    notice.value = `${asset.originalName} 已上传，公开链接已生效。`
    await load(true)
    await openDetail(asset)
  }
  catch (error) {
    notice.value = error instanceof Error ? error.message : '上传失败。'
  }
  finally {
    uploading.value = false
  }
}

async function copyUrl(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(absoluteMediaUrl(value))
    notice.value = '公开链接已复制。'
  }
  catch {
    notice.value = '浏览器未允许复制，请打开链接后从地址栏复制。'
  }
}

watch(() => route.query.q, (value) => {
  query.value = typeof value === 'string' ? value : ''
  void load(true)
}, { immediate: true })
void loadStorageStatus()
onBeforeUnmount(() => { if (searchTimer) clearTimeout(searchTimer) })
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="内容资源" title="媒体库" description="上传 PNG、JPEG 或 WebP；成功后公开链接立即生效，内容发布状态只控制页面是否展示。">
      <template #actions>
        <input ref="uploadInput" class="sr-only" type="file" aria-label="选择要上传的图片" :accept="ACCEPTED_TYPES" @change="handleFileSelection">
        <button v-if="canWrite" class="button button--primary" type="button" :disabled="uploading || !canUpload" @click="uploadInput?.click()">
          <UploadCloud :size="16" />{{ uploading ? '上传中…' : '上传图片' }}
        </button>
      </template>
    </PageHeader>

    <section v-if="storageConfigured === false" class="panel media-storage-warning" role="status">
      <div><strong>对象存储尚未配置</strong><p>平台其余功能可继续使用；媒体上传会保持禁用，直到通过系统设置完成连接测试并保存。</p></div>
      <button class="button button--primary" type="button" @click="router.push('/settings/object-storage')">前往对象存储设置</button>
    </section>

    <section class="panel media-toolbar" aria-label="媒体筛选">
      <label class="media-toolbar__search"><Search :size="16" /><span class="sr-only">搜索媒体</span><input v-model="query" type="search" placeholder="按原文件名搜索" @input="scheduleSearch"></label>
      <button class="button button--quiet" type="button" @click="load(true)"><RefreshCcw :size="15" />刷新</button>
    </section>

    <p v-if="notice" class="media-notice" role="status">{{ notice }}</p>
    <DataStatePanel
      v-if="pageState !== 'ready'"
      :state="pageState === 'loading' ? 'loading' : pageState === 'forbidden' ? 'forbidden' : pageState === 'error' ? 'error' : 'empty'"
      :title="pageState === 'error' ? errorMessage : pageState === 'empty' ? (hasFilters ? '没有匹配的媒体' : '媒体库为空') : ''"
      :description="pageState === 'empty' && !hasFilters ? '上传首张图片后即可获得公开链接。' : ''"
      @retry="load(true)"
    />

    <section v-else class="panel table-panel">
      <div class="data-table-wrap"><table class="data-table">
        <caption class="sr-only">媒体资源列表，共 {{ total }} 条</caption>
        <thead><tr><th>资源</th><th>类型</th><th>大小</th><th>上传时间</th><th>操作</th></tr></thead>
        <tbody><tr v-for="asset in items" :key="asset.id">
          <td><button class="media-name" type="button" @click="openDetail(asset)"><FileImage :size="16" /><span><strong>{{ asset.originalName }}</strong><small>{{ asset.id }}</small></span></button></td>
          <td>{{ asset.mediaType }}</td><td>{{ formatBytes(asset.byteSize) }}</td><td>{{ formatDate(asset.createdAt) }}</td>
          <td class="media-actions"><button class="button button--quiet" type="button" @click="copyUrl(asset.publicUrl)"><Copy :size="14" />复制链接</button><a class="button button--quiet" :href="absoluteMediaUrl(asset.publicUrl)" target="_blank" rel="noopener"><ExternalLink :size="14" />打开</a></td>
        </tr></tbody>
      </table></div>
      <div class="media-pagination"><span>共 {{ total }} 条</span><div><button class="button button--quiet" :disabled="!cursorStack.length" @click="previousPage">上一页</button><button class="button button--quiet" :disabled="!nextCursor" @click="nextPage">下一页</button></div></div>
    </section>

    <div v-if="selected" class="media-drawer-backdrop" @click.self="closeDetail">
      <aside class="media-drawer" role="dialog" aria-modal="true" aria-labelledby="media-detail-title">
        <header><div><small>媒体详情</small><h2 id="media-detail-title">{{ selected.originalName }}</h2></div><button class="icon-button" type="button" aria-label="关闭媒体详情" @click="closeDetail"><X :size="18" /></button></header>
        <img :src="absoluteMediaUrl(selected.publicUrl)" :alt="selected.originalName">
        <dl><div><dt>公开地址</dt><dd><code>{{ absoluteMediaUrl(selected.publicUrl) }}</code></dd></div><div><dt>下载地址</dt><dd><code>{{ absoluteMediaUrl(selected.downloadUrl) }}</code></dd></div><div><dt>SHA-256</dt><dd><code>{{ selected.sha256 }}</code></dd></div><div><dt>上传者</dt><dd>{{ selected.uploadedBy }}</dd></div></dl>
        <div class="media-drawer__actions"><button class="button button--quiet" type="button" @click="copyUrl(selected.publicUrl)"><Copy :size="14" />复制公开地址</button><a class="button button--secondary" :href="absoluteMediaUrl(selected.downloadUrl)" target="_blank" rel="noopener"><ExternalLink :size="14" />下载</a></div>
        <section><h3>内容引用</h3><p v-if="detailState === 'loading'">正在读取引用…</p><p v-else-if="detailState === 'error'" class="media-error">{{ detailError }}</p><p v-else-if="!references.length">暂无已发布内容引用。</p><ul v-else><li v-for="reference in references" :key="`${reference.contentId}:${reference.contentRevision}:${reference.referencePath}`"><strong>{{ reference.contentTitle }}</strong><span>{{ reference.dependencyKind }} · 修订 {{ reference.contentRevision }} · {{ reference.referencePath }}</span></li></ul></section>
      </aside>
    </div>
  </div>
</template>

<style scoped>
@layer components {
.media-toolbar { display: flex; align-items: center; gap: .75rem; }.media-toolbar__search { display: flex; flex: 1; align-items: center; gap: .45rem; }.media-toolbar__search input { width: 100%; }.media-storage-warning { display: flex; align-items: center; justify-content: space-between; gap: 1rem; border-color: #d97706; }.media-storage-warning p { margin: .25rem 0 0; color: var(--text-secondary); font-size: 0.75rem; }.media-notice { margin: 0; padding: .6rem .8rem; border-radius: .5rem; background: var(--surface-info); font-size: .75rem; }.media-name { display: flex; align-items: center; gap: .5rem; padding: 0; border: 0; background: none; color: inherit; text-align: left; }.media-name span { display: grid; }.media-name small { color: var(--text-secondary); font-size: 0.75rem; }.media-actions { display: flex; flex-wrap: wrap; gap: .35rem; }.media-pagination { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .65rem .25rem 0; font-size: 0.75rem; }.media-pagination div { display: flex; gap: .4rem; }.media-drawer-backdrop { position: fixed; z-index: 50; inset: 0; background: rgb(11 38 48 / 35%); }.media-drawer { position: absolute; top: 0; right: 0; display: flex; width: min(34rem, 100%); height: 100%; flex-direction: column; gap: 1rem; padding: 1.25rem; overflow-y: auto; background: white; box-shadow: -20px 0 45px rgb(11 38 48 / 18%); }.media-drawer header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; }.media-drawer h2 { margin: .2rem 0 0; font-size: 1rem; }.media-drawer img { width: 100%; max-height: 20rem; object-fit: contain; border-radius: .5rem; background: var(--surface-subtle); }.media-drawer dl { display: grid; gap: .65rem; margin: 0; }.media-drawer dl div { display: grid; gap: .2rem; }.media-drawer dt { color: var(--text-secondary); font-size: 0.75rem; }.media-drawer dd { margin: 0; font-size: 0.75rem; }.media-drawer code { overflow-wrap: anywhere; }.media-drawer__actions { display: flex; flex-wrap: wrap; gap: .5rem; }.media-drawer section h3 { font-size: .8rem; }.media-drawer ul { display: grid; gap: .5rem; padding: 0; list-style: none; }.media-drawer li { display: grid; gap: .15rem; padding: .55rem; border: 1px solid var(--border-default); border-radius: .5rem; }.media-drawer li span { color: var(--text-secondary); font-size: 0.75rem; }.media-error { color: var(--text-danger, #b91c1c); }
}
</style>
