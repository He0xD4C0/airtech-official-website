<script setup lang="ts">
import RfqForm from '@/features/conversion/components/forms/RfqForm.vue'
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
    <RfqForm v-if="page.rfqType" :kind="page.rfqType" :product-context="page.productContext" />
  </main>
</template>
