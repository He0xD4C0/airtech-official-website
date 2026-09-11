<script setup lang="ts">
import type { ContentPreviewResponse } from '@airtek/contracts'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'

defineProps<{ preview: ContentPreviewResponse }>()
</script>

<template>
  <main id="main-content" class="content-preview-page">
    <aside class="content-preview-banner" aria-label="Preview status">
      <div class="shell content-preview-banner__inner">
        <strong>Preview · Not published</strong>
        <span>
          Revision {{ preview.content.publishedRevision }} · Draft · expires
          <time :datetime="preview.previewExpiresAt">{{ preview.previewExpiresAt }}</time>
        </span>
      </div>
    </aside>
    <PublicBlockRenderer
      :blocks="preview.content.composition.blocks"
      :projection="preview.content"
    />
  </main>
</template>
