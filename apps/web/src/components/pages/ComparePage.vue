<script setup lang="ts">
import CompareWorkspace from '@/components/product/CompareWorkspace.vue'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/types/content'

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
    <CompareWorkspace :product-families="page.productFamilies ?? []" />
  </main>
</template>
