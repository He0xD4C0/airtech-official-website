<script setup lang="ts">
import { computed } from 'vue'
import BannerCarousel from '@airtek/content-renderer/BannerCarousel.vue'
import { linkTargetHref, mediaAlt, mediaAssetHref } from '@/shared/types/projection'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/shared/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const isHome = computed(() => props.page.kind === 'home')
const contentBlocks = computed(() => (props.page.projection?.composition.blocks ?? [])
  .filter(block => !isHome.value || block.type !== 'hero'))
const slides = computed(() => {
  const projection = props.page.projection
  if (!isHome.value || !projection) return []
  return projection.composition.blocks.flatMap(block => block.type === 'hero' ? [{
    id: block.id,
    eyebrow: block.eyebrow ?? '',
    heading: block.heading ?? projection.title,
    lead: block.lead ?? projection.summary ?? '',
    image: mediaAssetHref(block.media, projection.resolvedMedia),
    alt: mediaAlt(block.media),
    actions: block.actions.flatMap(action => {
      const href = linkTargetHref(action.target, projection.resolvedLinks)
      return action.label && href ? [{ label: action.label, href }] : []
    }),
  }] : [])
})
</script>

<template>
  <main id="main-content">
    <BannerCarousel v-if="isHome" :slides="slides" />
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="contentBlocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section v-if="page.kind === 'home' && page.productFamilies?.length" class="section shell" aria-labelledby="database-product-families">
      <h2 id="database-product-families">Product families</h2>
      <div class="family-strip">
        <a v-for="family in page.productFamilies" :key="family.code" :href="`/en/products?family=${family.code}`">
          <strong>{{ family.name }}</strong>
          <small v-if="family.description">{{ family.description }}</small>
        </a>
      </div>
    </section>

  </main>
</template>
