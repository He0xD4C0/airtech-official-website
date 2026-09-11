<script setup lang="ts">
import ProductExplorer from '@/components/product/ProductExplorer.vue'
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
    <ProductExplorer
      :category="page.category"
      :families="page.productFamilies ?? []"
      :products="page.publishedProducts ?? []"
      :next-cursor="page.productNextCursor"
    />
  </main>
</template>
