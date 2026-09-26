<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import { downloadAssetHref, type PublicContentProjection } from '@/shared/types/projection'
import { useI18n } from 'vue-i18n'

type DownloadAssetValue = Extract<ContentBlock, { type: 'downloadAsset' }>

const props = defineProps<{ block: DownloadAssetValue; projection: PublicContentProjection }>()
const href = computed(() => downloadAssetHref(props.block.asset, props.projection.resolvedMedia))
const { locale, t } = useI18n({ useScope: 'global' })
</script>

<template>
  <section v-if="href" class="section shell download-asset" :lang="locale">
    <a class="button" :lang="block.label ? 'en' : locale" :href="href" download>{{ block.label || t('common.download') }}</a>
    <p v-if="block.description" lang="en">{{ block.description }}</p>
  </section>
</template>
