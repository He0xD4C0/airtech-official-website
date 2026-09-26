<script setup lang="ts">
import CompareWorkspace from '@/features/compare/components/CompareWorkspace.vue'
import { PublicBlockRenderer } from '@/features/content'
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
    <CompareWorkspace :product-families="page.productFamilies ?? []" />
  </main>
</template>
