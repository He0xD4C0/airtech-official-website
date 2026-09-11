<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import PageHero from '@/components/common/PageHero.vue'
import type { Breadcrumb } from '@/types/content'
import { isDecorative, linkTargetHref, mediaAlt, mediaAssetHref, type PublicContentProjection } from '@/types/projection'

type HeroBlockValue = Extract<ContentBlock, { type: 'hero' }>

const props = defineProps<{
  block: HeroBlockValue
  projection: PublicContentProjection
  breadcrumbs?: Breadcrumb[]
}>()

const actions = (): Array<{ label: string; href: string }> => props.block.actions.flatMap((action) => {
  const href = linkTargetHref(action.target, props.projection.resolvedLinks)
  return action.label && href ? [{ label: action.label, href }] : []
})
const mediaHref = computed(() => mediaAssetHref(props.block.media))
</script>

<template>
  <PageHero
    :eyebrow="block.eyebrow ?? ''"
    :title="block.heading ?? projection.title"
    :description="block.lead ?? projection.summary ?? ''"
    :breadcrumbs="breadcrumbs"
  />
  <figure v-if="mediaHref" class="section shell hero-block__media">
    <img
      :src="mediaHref"
      :alt="mediaAlt(block.media)"
      :aria-hidden="isDecorative(block.media) ? 'true' : undefined"
      decoding="async"
    />
  </figure>
  <div v-if="actions().length" class="section shell button-row">
    <a v-for="action in actions()" :key="action.href" class="button" :href="action.href">{{ action.label }}</a>
  </div>
</template>
