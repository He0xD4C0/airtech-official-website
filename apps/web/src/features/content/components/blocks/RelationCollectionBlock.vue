<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import type { PublicContentProjection } from '@/shared/types/projection'

type RelationCollectionValue = Extract<ContentBlock, { type: 'relationCollection' }>

const props = defineProps<{
  block: RelationCollectionValue
  projection: PublicContentProjection
}>()

const cards = computed(() => props.projection.resolvedRelations.filter((card) => (
  props.block.relationIds.includes(card.relationId)
)))
</script>

<template>
  <section v-if="cards.length" class="section shell relation-collection">
    <h2 v-if="block.heading">{{ block.heading }}</h2>
    <div :class="['card-grid', 'collection-grid', `relation-collection--${block.presentation}`]">
      <article v-for="card in cards" :key="card.relationId" class="card collection-card">
        <p v-if="card.eyebrow" class="eyebrow">{{ card.eyebrow }}</p>
        <h3><a :href="card.href">{{ card.title }}</a></h3>
        <p v-if="card.summary">{{ card.summary }}</p>
      </article>
    </div>
  </section>
</template>
