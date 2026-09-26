<script setup lang="ts">
import { computed, ref } from 'vue'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { trackAnalyticsEvent } from '@/features/analytics'
import type { PublicPageModel } from '@/shared/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const query = ref('')
const entries = computed(() => props.page.entries ?? [])
const visibleEntries = computed(() => {
  if (props.page.collection !== 'articles') return entries.value
  const needle = query.value.trim().toLowerCase()
  if (!needle) return entries.value
  return entries.value.filter((entry) => (
    `${entry.title} ${entry.summary} ${(entry.tags ?? []).join(' ')}`.toLowerCase().includes(needle)
  ))
})

function trackArticleSearch(): void {
  const needle = query.value.trim()
  if (!needle) return
  void trackAnalyticsEvent('internalSearch', {
    queryLength: needle.length,
    resultCount: visibleEntries.value.length,
  })
}
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section class="section shell">
      <div v-if="page.collection === 'articles' && entries.length" class="filter-panel article-filter-panel">
        <label>
          <span>Search published articles</span>
          <input v-model="query" type="search" placeholder="Search title, summary or category" @change="trackArticleSearch">
        </label>
        <p class="result-count" role="status">{{ visibleEntries.length }} published {{ visibleEntries.length === 1 ? 'article' : 'articles' }}</p>
      </div>
      <div class="card-grid collection-grid">
        <article v-for="(entry, index) in visibleEntries" :key="entry.slug" class="card collection-card">
          <span class="card-number">{{ String(index + 1).padStart(2, '0') }}</span>
          <p v-if="entry.eyebrow" class="eyebrow">{{ entry.eyebrow }}</p>
          <h2><a :href="entry.href">{{ entry.title }}</a></h2><p v-if="entry.summary">{{ entry.summary }}</p>
          <span v-if="entry.status" class="status pending">{{ entry.status }}</span>
          <ul v-if="entry.tags" class="tag-list"><li v-for="tag in entry.tags" :key="tag">{{ tag }}</li></ul>
          <a class="text-link" :href="entry.href">Open {{ page.collection === 'articles' ? 'article' : 'page' }} →</a>
        </article>
      </div>
      <div v-if="!entries.length" class="empty-state large">
        <p class="eyebrow">No published entries</p>
        <h2>No indexable records are available in this collection.</h2>
        <p>Drafts, placeholders and non-indexable records are not mixed into the public listing.</p>
      </div>
      <div v-else-if="!visibleEntries.length" class="empty-state">
        <h2>No matching published article</h2>
        <p>Try a broader title, summary or category term.</p>
        <button class="button secondary" type="button" @click="query = ''">Clear search</button>
      </div>
    </section>
  </main>
</template>
