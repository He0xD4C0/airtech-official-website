<script setup lang="ts">
import RfqForm from '@/components/forms/RfqForm.vue'
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
    <RfqForm v-if="page.rfqType" :kind="page.rfqType" :product-context="page.productContext" />
  </main>
</template>
