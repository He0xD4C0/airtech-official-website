<script setup lang="ts">
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/shared/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
  </main>
</template>
