<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import CompareWorkspace from '@/components/product/CompareWorkspace.vue'
import PageSlotSections from './PageSlotSections.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <CompareWorkspace :product-families="page.productFamilies ?? []" />
    <PageSlotSections :sections="page.sections" />
  </main>
</template>
