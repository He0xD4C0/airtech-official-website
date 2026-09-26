<script setup lang="ts">
import ProductExplorer from '@/features/catalog/components/ProductExplorer.vue'
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
    <ProductExplorer
      :category="page.category"
      :families="page.productFamilies ?? []"
      :products="page.publishedProducts ?? []"
      :next-cursor="page.productNextCursor"
      :total="page.productTotal"
      :family-counts="page.productFamilyCounts"
      :motor-technology-counts="page.productMotorTechnologyCounts"
      :initial-state="page.catalogState"
    />
  </main>
</template>
