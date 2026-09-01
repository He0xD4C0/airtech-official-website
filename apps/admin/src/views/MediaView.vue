<script setup lang="ts">
import { ref } from 'vue'
import { FileArchive, FileImage, FileText, FolderOpen, Grid2X2, Info, List, MoreHorizontal, Search, UploadCloud } from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { mockApiEnabled } from '@/services/adminApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const view = ref<'grid' | 'list'>('grid')
const dragActive = ref(false)

const assets = mockApiEnabled ? [
  { id: 'demo-media-01', name: 'placeholder-product-image.webp', kind: 'image', detail: '1600 × 1200 · Demo only', status: 'approved', refs: 0 },
  { id: 'demo-media-02', name: 'draft-document.pdf', kind: 'pdf', detail: 'PDF · Demo only', status: 'quarantine', refs: 0 },
  { id: 'demo-media-03', name: 'pending-cad-resource.zip', kind: 'archive', detail: 'ZIP · Demo only', status: 'pending', refs: 0 },
] : []

const icon = { image: FileImage, pdf: FileText, archive: FileArchive }

function mockUpload(): void {
  dragActive.value = false
  ui.toast(mockApiEnabled ? '开发演示未上传文件' : '媒体适配器尚未配置', mockApiEnabled ? '正式上传将使用分片、校验和、隔离扫描与版本记录。' : '没有可用的对象存储与扫描 API，未接收该文件。', 'info')
}
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="ASSET LIBRARY" title="媒体中心" description="管理网站图片、PDF、CAD、证书与其他公开资源；RFQ 不接受附件。">
      <template #actions><button class="button button--primary" type="button" @click="mockUpload"><UploadCloud :size="16" />上传资源</button></template>
    </PageHeader>

    <section class="media-layout">
      <aside class="media-folders panel">
        <p>资源目录</p>
        <button class="is-active" type="button"><FolderOpen :size="17" />全部资源<span>{{ assets.length }}</span></button>
        <button type="button"><FolderOpen :size="17" />产品图片<span>{{ mockApiEnabled ? 1 : 0 }}</span></button>
        <button type="button"><FolderOpen :size="17" />Downloads<span>{{ mockApiEnabled ? 2 : 0 }}</span></button>
        <button type="button"><FolderOpen :size="17" />证书<span>0</span></button>
        <button type="button"><FolderOpen :size="17" />品牌资产<span>0</span></button>
        <div class="media-note"><Info :size="16" /><p>仓库尚无获批 Logo，生产界面继续使用文字标识，不重绘图形。</p></div>
      </aside>

      <section class="panel media-browser">
        <div class="table-toolbar">
          <label class="search-field"><Search :size="17" /><input placeholder="搜索文件名、标签或引用" /></label>
          <div class="view-toggle" role="group" aria-label="视图模式"><button type="button" :class="{ 'is-active': view === 'grid' }" @click="view = 'grid'"><Grid2X2 :size="16" /></button><button type="button" :class="{ 'is-active': view === 'list' }" @click="view = 'list'"><List :size="16" /></button></div>
        </div>

        <button class="upload-zone" :class="{ 'is-dragging': dragActive }" type="button" @dragenter.prevent="dragActive = true" @dragleave.prevent="dragActive = false" @dragover.prevent @drop.prevent="mockUpload" @click="mockUpload">
          <span><UploadCloud :size="24" /></span><div><strong>拖放文件到此处，或点击选择</strong><p>上传后先进入隔离区，扫描通过且获批后才能被公开内容引用。</p></div>
        </button>

        <div class="media-grid" :class="{ 'media-grid--list': view === 'list' }">
          <article v-for="asset in assets" :key="asset.id" class="media-card">
            <div class="media-card__preview"><component :is="icon[asset.kind as keyof typeof icon]" :size="34" /><StatusBadge :label="asset.status === 'approved' ? '已批准' : asset.status === 'quarantine' ? '隔离中' : '待处理'" :tone="asset.status === 'approved' ? 'success' : asset.status === 'quarantine' ? 'warning' : 'neutral'" /></div>
            <div class="media-card__body"><strong>{{ asset.name }}</strong><p>{{ asset.detail }}</p><span>{{ asset.refs }} 个内容引用 · 演示资源</span></div>
            <button type="button" class="icon-button" aria-label="资源操作"><MoreHorizontal :size="18" /></button>
          </article>
          <div v-if="!assets.length" class="module-empty"><FolderOpen :size="27" /><h2>对象存储适配器尚未配置</h2><p>媒体 API 可用前不会展示或接收伪造资源。</p></div>
        </div>
      </section>
    </section>
  </div>
</template>
