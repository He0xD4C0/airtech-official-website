<script setup lang="ts">
import { computed } from 'vue'
import PageHero from '@/components/common/PageHero.vue'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'
import ArticlePublicationMeta from '@/components/content/ArticlePublicationMeta.vue'
import DownloadResourcePanel from '@/components/content/DownloadResourcePanel.vue'
import ContactForm from '@/components/forms/ContactForm.vue'
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

    <section v-if="page.kind === 'contact'" class="section shell published-contact" aria-labelledby="published-contact-form-title">
      <div>
        <p class="eyebrow">General contact</p>
        <h2 id="published-contact-form-title">Send a message</h2>
        <p>For product selection or project requirements, use the structured Request a Quote path so the engineering context stays attached.</p>
        <a class="button secondary" href="/en/request-a-quote">Go to Request a Quote</a>
      </div>
      <ContactForm />
    </section>

    <section v-if="page.kind === 'legal' && page.canonicalPath.endsWith('/cookie-settings')" class="section shell published-cookie-control" aria-labelledby="published-cookie-title">
      <h2 id="published-cookie-title">Cookie preference</h2>
      <button class="button" type="button" @click="openCookieSettings">Review cookie preference</button>
    </section>
  </main>
</template>
