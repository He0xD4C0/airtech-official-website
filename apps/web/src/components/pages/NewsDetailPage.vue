<script setup lang="ts">
import PageHero from '@/components/common/PageHero.vue'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'
import CallToAction from '@/components/common/CallToAction.vue'
import PageSlotSections from './PageSlotSections.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
function visibleDate(value: string): string { return value.slice(0, 10) }
</script>

<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <section v-if="page.publishedContent" class="section shell published-content">
      <p v-if="page.newsMetadata?.author || page.newsMetadata?.publishedAt || page.newsMetadata?.category" class="published-content-meta">
        <span v-if="page.newsMetadata?.category">{{ page.newsMetadata.category }}</span>
        <span v-if="page.newsMetadata?.author">{{ page.newsMetadata.author }}</span>
        <time v-if="page.newsMetadata?.publishedAt" :datetime="page.newsMetadata.publishedAt">{{ visibleDate(page.newsMetadata.publishedAt) }}</time>
      </p>
      <StructuredContentRenderer :document="page.publishedContent.body" />
    </section>
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
