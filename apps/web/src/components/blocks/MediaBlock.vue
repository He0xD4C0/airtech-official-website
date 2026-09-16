<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import { isDecorative, mediaAlt, mediaAssetHref, type PublicContentProjection } from '@/types/projection'

type MediaBlockValue = Extract<ContentBlock, { type: 'media' }>

const props = defineProps<{ block: MediaBlockValue; projection: PublicContentProjection }>()
const src = computed(() => mediaAssetHref(props.block.media, props.projection.resolvedMedia))
</script>

<template>
  <figure v-if="src" :class="['section shell media-block', `media-block--${block.layout}`]">
    <img
      :src="src"
      :alt="mediaAlt(block.media)"
      :aria-hidden="isDecorative(block.media) ? 'true' : undefined"
      loading="lazy"
      decoding="async"
    />
    <figcaption v-if="block.caption">{{ block.caption }}</figcaption>
  </figure>
</template>
