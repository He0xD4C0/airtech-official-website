<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import { downloadAssetHref, type PublicContentProjection } from '@/shared/types/projection'

type DownloadAssetValue = Extract<ContentBlock, { type: 'downloadAsset' }>

const props = defineProps<{ block: DownloadAssetValue; projection: PublicContentProjection }>()
const href = computed(() => downloadAssetHref(props.block.asset, props.projection.resolvedMedia))
</script>

<template>
  <section v-if="href" class="section shell download-asset">
    <a class="button" :href="href" download>{{ block.label || 'Download' }}</a>
    <p v-if="block.description">{{ block.description }}</p>
  </section>
</template>
