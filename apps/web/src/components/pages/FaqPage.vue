<script setup lang="ts">
import type { PublicPageModel } from '@/types/content'
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'
defineProps<{ page: PublicPageModel }>()
</script>
<template>
  <main id="main-content">
    <template v-if="page.projection">
      <PublicBlockRenderer
        :blocks="page.projection.composition.blocks"
        :projection="page.projection"
        :breadcrumbs="page.breadcrumbs"
      />
      <nav class="shell category-tabs" aria-label="FAQ categories"><a href="/en/resources/faqs" :aria-current="!page.slug ? 'page' : undefined">All</a><a v-for="entry in page.entries" :key="entry.slug" :href="entry.href" :aria-current="page.slug === entry.slug ? 'page' : undefined">{{ entry.title }}</a></nav>
    </template>
  </main>
</template>
