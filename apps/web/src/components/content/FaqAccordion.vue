<script setup lang="ts">
import { computed, ref } from 'vue'
import StructuredContentRenderer from './StructuredContentRenderer'
import { trackAnalyticsEvent } from '@/lib/analytics'
import type { PublishedFaqItem } from '@/lib/publishedContent'

const props = defineProps<{ items: PublishedFaqItem[] }>()
const query = ref('')

const visibleItems = computed(() => props.items.filter((item) => {
  const text = `${item.question} ${item.answer}`.toLowerCase()
  return text.includes(query.value.trim().toLowerCase())
}))

function trackExpanded(event: Event, item: PublishedFaqItem): void {
  if (!(event.currentTarget instanceof HTMLDetailsElement) || !event.currentTarget.open) return
  void trackAnalyticsEvent('faqExpanded', { faqId: item.id })
}
</script>

<template>
  <section class="section shell faq-section">
    <label class="faq-search"><span>Search questions</span><input v-model="query" type="search" placeholder="Search by topic or keyword"></label>
    <div class="faq-list">
      <details v-for="item in visibleItems" :key="item.id" @toggle="trackExpanded($event, item)">
        <summary :id="`${item.id}-question`">{{ item.question }}<span aria-hidden="true">+</span></summary>
        <div role="region" :aria-labelledby="`${item.id}-question`">
          <StructuredContentRenderer :document="item.answerDocument" />
        </div>
      </details>
    </div>
    <div v-if="!visibleItems.length" class="empty-state"><h2>No matching questions</h2><p>Try a broader search or browse another FAQ category.</p></div>
  </section>
</template>
