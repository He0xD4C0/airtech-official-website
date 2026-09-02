<script setup lang="ts">
import { FolderOpen, Info, UploadCloud } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="ASSET LIBRARY" title="媒体中心" description="管理网站图片、PDF、CAD、证书与其他公开资源；RFQ 不接受附件。">
      <template #actions><button class="button button--primary" type="button" disabled><UploadCloud :size="16" />上传资源</button></template>
    </PageHeader>

    <section class="media-layout">
      <aside class="media-folders panel">
        <p>资源目录</p>
        <button class="is-active" type="button"><FolderOpen :size="17" />全部资源<span>—</span></button>
        <button type="button"><FolderOpen :size="17" />产品图片<span>—</span></button>
        <button type="button"><FolderOpen :size="17" />Downloads<span>—</span></button>
        <button type="button"><FolderOpen :size="17" />证书<span>—</span></button>
        <button type="button"><FolderOpen :size="17" />品牌资产<span>—</span></button>
        <div class="media-note"><Info :size="16" /><p>仓库尚无获批 Logo，生产界面继续使用文字标识，不重绘图形。</p></div>
      </aside>

      <DataStatePanel
        state="empty"
        title="媒体 API 尚未提供"
        description="对象存储、扫描与媒体列表端点可用前，后台不会接收文件或展示伪造资源。"
      />
    </section>
  </div>
</template>
