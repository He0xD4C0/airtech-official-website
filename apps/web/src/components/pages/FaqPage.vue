<script setup lang="ts">
import { computed } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import FaqAccordion from '@/components/content/FaqAccordion.vue'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'
import { hasRenderableRichText } from '@/lib/richText'
import { extractPublishedFaqContent } from '@/lib/publishedContent'
import type { PublicPageModel } from '@/types/content'
import PageSlotSections from './PageSlotSections.vue'
const props = defineProps<{ page: PublicPageModel }>()
const faqContent = computed(() => {
  const content = props.page.publishedContent
  if (content?.status !== 'published' || content.isPlaceholder) return extractPublishedFaqContent(undefined)
  return extractPublishedFaqContent(content.body)
})
const hasEditorialContent = computed(() => hasRenderableRichText(faqContent.value.editorialDocument))
</script>
<template>
  <main id="main-content">
    <PageHero :eyebrow="page.eyebrow" :title="page.title" :description="page.description" :breadcrumbs="page.breadcrumbs" />
    <nav class="shell category-tabs" aria-label="FAQ categories"><a href="/en/resources/faqs" :aria-current="!page.slug ? 'page' : undefined">All</a><a v-for="entry in page.entries" :key="entry.slug" :href="entry.href" :aria-current="page.slug === entry.slug ? 'page' : undefined">{{ entry.title }}</a></nav>
    <section v-if="hasEditorialContent && faqContent.editorialDocument" class="section shell published-editorial" aria-label="Published editorial content">
      <StructuredContentRenderer :document="faqContent.editorialDocument" />
    </section>
    <FaqAccordion v-if="faqContent.items.length" :items="faqContent.items" />
    <section v-else class="section shell">
      <div class="empty-state large">
        <p class="eyebrow">Published FAQ content</p>
        <h2>{{ page.slug ? 'No complete published questions are available in this category.' : 'No complete published questions are available.' }}</h2>
        <p>Only question-and-answer pairs from the published CMS record are shown. Draft, incomplete and demonstration copy is excluded.</p>
      </div>
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
