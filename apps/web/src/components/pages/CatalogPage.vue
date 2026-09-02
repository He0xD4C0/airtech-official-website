<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import ProductExplorer from '@/components/product/ProductExplorer.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PublishedEditorialSection from '@/components/content/PublishedEditorialSection.vue'
import PageSlotSections from './PageSlotSections.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs">
    </PageHero>
    <PublishedEditorialSection :content="page.publishedContent" />
    <ProductExplorer
      :category="page.category"
      :families="page.productFamilies ?? []"
      :products="page.publishedProducts ?? []"
      :next-cursor="page.productNextCursor"
    />
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
