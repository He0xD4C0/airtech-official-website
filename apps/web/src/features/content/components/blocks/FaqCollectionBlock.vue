<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import StructuredContentRenderer from '@/features/content/components/content/StructuredContentRenderer'
import type { PublicContentProjection } from '@/shared/types/projection'

type FaqCollectionValue = Extract<ContentBlock, { type: 'faqCollection' }>

const props = defineProps<{
  block: FaqCollectionValue
  projection: PublicContentProjection
}>()

const items = computed(() => (
  props.projection.typeFields.type === 'faq' ? props.projection.typeFields.items : []
))

</script>

<template>
  <section v-if="items.length" class="section shell faq-section">
    <h2 v-if="block.heading">{{ block.heading }}</h2>
    <div class="faq-list">
      <details v-for="item in items" :key="item.id">
        <summary>{{ item.question }}</summary>
        <StructuredContentRenderer :document="item.answer" />
      </details>
    </div>
  </section>
</template>
