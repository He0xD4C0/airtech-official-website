<script setup lang="ts">
import { computed } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'
import ArticlePublicationMeta from '@/components/content/ArticlePublicationMeta.vue'
import DownloadResourcePanel from '@/components/content/DownloadResourcePanel.vue'
import CallToAction from '@/components/common/CallToAction.vue'
import PageSlotSections from './PageSlotSections.vue'
import { extractPublishedArticleMetadata } from '@/lib/publishedContent'
import type { PublicPageModel } from '@/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const articleMetadata = computed(() => extractPublishedArticleMetadata(props.page.publishedContent))
const visibleUpdatedDate = computed(() => {
  if (props.page.publishedContent?.kind !== 'caseStudy') return undefined
  return props.page.publishedContent?.updatedAt.match(/^\d{4}-\d{2}-\d{2}/u)?.[0]
})

function openCookieSettings(): void {
  window.dispatchEvent(new CustomEvent('airtek:open-cookie-settings'))
}
</script>

<template>
  <main id="main-content">
    <PageHero
      :eyebrow="page.eyebrow"
      :title="page.title"
      :description="page.description"
      :breadcrumbs="page.breadcrumbs"
      :compact="page.kind === 'legal'"
    />
    <section v-if="props.page.publishedContent" class="section shell published-content">
      <ArticlePublicationMeta v-if="props.page.publishedContent.kind === 'article'" :metadata="articleMetadata" />
      <p v-if="visibleUpdatedDate" class="published-content-meta">
        Last updated <time :datetime="props.page.publishedContent.updatedAt">{{ visibleUpdatedDate }}</time>
      </p>
      <StructuredContentRenderer :document="props.page.publishedContent.body" />
      <DownloadResourcePanel v-if="props.page.publishedContent.kind === 'download'" :page="page" />
    </section>

    <section v-if="page.kind === 'legal' && page.canonicalPath.endsWith('/cookie-settings')" class="section shell published-cookie-control" aria-labelledby="published-cookie-title">
      <h2 id="published-cookie-title">Cookie preference</h2>
      <button class="button" type="button" @click="openCookieSettings">Review cookie preference</button>
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
