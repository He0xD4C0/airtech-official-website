<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import { isDecorative, mediaAlt, mediaAssetHref, type PublicContentProjection } from '@/types/projection'
import MediaLightbox from '@/components/common/MediaLightbox.vue'

type MediaBlockValue = Extract<ContentBlock, { type: 'media' }>

const props = defineProps<{ block: MediaBlockValue; projection: PublicContentProjection }>()
const src = computed(() => mediaAssetHref(props.block.media, props.projection.resolvedMedia))
const originalSrc = computed(() => mediaAssetHref(props.block.media, props.projection.resolvedMedia, 'original'))
</script>

<template>
  <figure v-if="src && isDecorative(block.media)" :class="['section shell media-block', `media-block--${block.layout}`]">
    <img
      :src="src"
      :alt="mediaAlt(block.media)"
      :aria-hidden="isDecorative(block.media) ? 'true' : undefined"
      loading="lazy"
      decoding="async"
    />
    <figcaption v-if="block.caption">{{ block.caption }}</figcaption>
  </figure>
  <MediaLightbox
    v-else-if="src && originalSrc"
    :class="['section shell media-block', `media-block--${block.layout}`]"
    :preview-src="src"
    :original-src="originalSrc"
    :alt="mediaAlt(block.media)"
    :caption="block.caption"
  />
</template>
