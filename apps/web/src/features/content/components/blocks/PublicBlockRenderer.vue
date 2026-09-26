<script setup lang="ts">
import type { ContentBlock } from '@airtek/contracts'
import BodyBlock from '@/features/content/components/blocks/BodyBlock.vue'
import ContactBlockBlock from '@/features/content/components/blocks/ContactBlockBlock.vue'
import CtaBlock from '@/features/content/components/blocks/CtaBlock.vue'
import DownloadAssetBlock from '@/features/content/components/blocks/DownloadAssetBlock.vue'
import EvidenceBlock from '@/features/content/components/blocks/EvidenceBlock.vue'
import FaqCollectionBlock from '@/features/content/components/blocks/FaqCollectionBlock.vue'
import FeatureGridBlock from '@/features/content/components/blocks/FeatureGridBlock.vue'
import HeroBlock from '@/features/content/components/blocks/HeroBlock.vue'
import MediaBlock from '@/features/content/components/blocks/MediaBlock.vue'
import RelationCollectionBlock from '@/features/content/components/blocks/RelationCollectionBlock.vue'
import type { Breadcrumb } from '@/shared/types/content'
import type { PublicContentProjection } from '@/shared/types/projection'

defineProps<{
  blocks: ContentBlock[]
  projection: PublicContentProjection
  breadcrumbs?: Breadcrumb[]
}>()
</script>

<template>
  <template v-for="block in blocks" :key="block.id">
    <HeroBlock
      v-if="block.type === 'hero'"
      :block="block"
      :projection="projection"
      :breadcrumbs="breadcrumbs"
    />
    <BodyBlock v-else-if="block.type === 'body'" :projection="projection" />
    <MediaBlock v-else-if="block.type === 'media'" :block="block" :projection="projection" />
    <FeatureGridBlock v-else-if="block.type === 'featureGrid'" :block="block" :projection="projection" />
    <EvidenceBlock v-else-if="block.type === 'evidence'" :block="block" />
    <CtaBlock v-else-if="block.type === 'cta'" :block="block" :projection="projection" />
    <RelationCollectionBlock
      v-else-if="block.type === 'relationCollection'"
      :block="block"
      :projection="projection"
    />
    <FaqCollectionBlock
      v-else-if="block.type === 'faqCollection'"
      :block="block"
      :projection="projection"
    />
    <DownloadAssetBlock v-else-if="block.type === 'downloadAsset'" :block="block" :projection="projection" />
    <ContactBlockBlock
      v-else-if="block.type === 'contactBlock'"
      :block="block"
      :projection="projection"
    />
  </template>
</template>
