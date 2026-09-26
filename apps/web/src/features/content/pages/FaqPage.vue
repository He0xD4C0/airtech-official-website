<script setup lang="ts">
import type { PublicPageModel } from '@/shared/types/content'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { useI18n } from 'vue-i18n'
defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
</script>
<template>
  <main id="main-content">
    <template v-if="page.projection">
      <PublicBlockRenderer
        :blocks="page.projection.composition.blocks"
        :projection="page.projection"
        :breadcrumbs="page.breadcrumbs"
      />
      <nav class="shell category-tabs" :aria-label="t('content.faqCategories')" :lang="locale"><a href="/en/resources/faqs" :aria-current="!page.slug ? 'page' : undefined">{{ t('content.all') }}</a><a v-for="entry in page.entries" :key="entry.slug" :href="entry.href" :aria-current="page.slug === entry.slug ? 'page' : undefined" lang="en">{{ entry.title }}</a></nav>
    </template>
  </main>
</template>
