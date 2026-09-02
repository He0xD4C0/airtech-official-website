<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import SelectorWorkspace from '@/components/product/SelectorWorkspace.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PageSlotSections from './PageSlotSections.vue'
import type { PublicPageModel } from '@/types/content'
defineProps<{ page: PublicPageModel }>()
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <PublishedEditorialSection :content="page.publishedContent" />
    <SelectorWorkspace :product-families="page.productFamilies ?? []" :motor-technologies="page.motorTechnologies ?? []" />
    <PageSlotSections :sections="page.sections" />
    <CallToAction
      v-if="page.primaryCta"
      :eyebrow="page.primaryCta.eyebrow"
      :title="page.primaryCta.title"
      :description="page.primaryCta.description"
      :href="page.primaryCta.href"
      :label="page.primaryCta.label"
    />
  </main>
</template>
