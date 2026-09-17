<script setup lang="ts">
import type { ProductMediaGalleryItem } from '@airtek/contracts'
import MediaLightbox from '@/components/common/MediaLightbox.vue'
import { publicMediaHref } from '@/types/projection'

defineProps<{ items: ProductMediaGalleryItem[] }>()

function displayUrl(assetId: string): string {
  return publicMediaHref(`/api/public/v1/media/${assetId}`)
}

function originalUrl(assetId: string): string {
  return publicMediaHref(`/api/public/v1/media/${assetId}/download`)
}
</script>

<template>
  <section v-if="items.length" class="section shell product-gallery" aria-labelledby="product-gallery-title">
    <h2 id="product-gallery-title">Product gallery</h2>
    <div class="product-gallery__grid">
      <MediaLightbox
        v-for="(item, index) in items"
        :key="item.assetId"
        :class="{ 'product-gallery__primary': index === 0 }"
        :preview-src="displayUrl(item.assetId)"
        :original-src="originalUrl(item.assetId)"
        :alt="item.altText"
      />
    </div>
  </section>
</template>

<style scoped>
@layer components {
  .product-gallery h2 { margin-block-start: 0; }
  .product-gallery__grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 15rem), 1fr)); gap: var(--space-4); }
  .product-gallery :deep(.media-lightbox__trigger img) { aspect-ratio: 4 / 3; border-radius: var(--airtek-radius-md); object-fit: cover; }
  .product-gallery__primary { grid-column: 1 / -1; }
}
</style>
