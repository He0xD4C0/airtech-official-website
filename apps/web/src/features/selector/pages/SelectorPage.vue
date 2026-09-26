<script setup lang="ts">
import SelectorWorkspace from '@/features/selector/components/SelectorWorkspace.vue'
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
    <SelectorWorkspace :product-families="page.productFamilies ?? []" :motor-technologies="page.motorTechnologies ?? []" />
  </main>
</template>
