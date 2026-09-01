<script setup lang="ts">
import type { ContentPreviewResponse } from '@airtek/contracts'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'

defineProps<{ preview: ContentPreviewResponse }>()
</script>

<template>
  <main id="main-content" class="content-preview-page">
    <aside class="content-preview-banner" aria-label="Preview status">
      <div class="shell content-preview-banner__inner">
        <strong>Preview · Not published</strong>
        <span>
          Revision {{ preview.content.currentRevision }} · {{ preview.content.status }} · expires
          <time :datetime="preview.previewExpiresAt">{{ preview.previewExpiresAt }}</time>
        </span>
      </div>
    </aside>
    <header class="content-preview-hero">
      <div class="shell">
        <p class="eyebrow">Private content preview</p>
        <h1>{{ preview.content.title }}</h1>
        <p v-if="preview.content.summary" class="content-preview-summary">{{ preview.content.summary }}</p>
      </div>
    </header>
    <section class="section shell content-preview-body" aria-label="Preview content">
      <StructuredContentRenderer :document="preview.content.body" />
    </section>
  </main>
</template>
