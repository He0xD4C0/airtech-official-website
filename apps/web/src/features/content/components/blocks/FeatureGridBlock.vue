<script setup lang="ts">
import type { ContentBlock } from '@airtek/contracts'
import { isDecorative, mediaAlt, mediaAssetHref, type PublicContentProjection } from '@/shared/types/projection'

type FeatureGridBlockValue = Extract<ContentBlock, { type: 'featureGrid' }>

const props = defineProps<{ block: FeatureGridBlockValue; projection: PublicContentProjection }>()

function iconHref(item: FeatureGridBlockValue['items'][number]): string | undefined {
  return mediaAssetHref(item.icon, props.projection.resolvedMedia)
}
</script>

<template>
  <section class="section shell feature-grid">
    <h2 v-if="block.heading">{{ block.heading }}</h2>
    <div class="card-grid collection-grid">
      <article v-for="item in block.items" :key="item.id" class="card collection-card">
        <img
          v-if="iconHref(item)"
          :src="iconHref(item)"
          :alt="mediaAlt(item.icon)"
          :aria-hidden="isDecorative(item.icon) ? 'true' : undefined"
          loading="lazy"
          decoding="async"
        />
        <h3>{{ item.title }}</h3>
        <p v-if="item.description">{{ item.description }}</p>
      </article>
    </div>
  </section>
</template>
