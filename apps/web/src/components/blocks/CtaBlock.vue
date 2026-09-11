<script setup lang="ts">
import { computed } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import CallToAction from '@/components/common/CallToAction.vue'
import { linkTargetHref, type PublicContentProjection } from '@/types/projection'

type CtaBlockValue = Extract<ContentBlock, { type: 'cta' }>

const props = defineProps<{
  block: CtaBlockValue
  projection: PublicContentProjection
}>()

const href = computed(() => linkTargetHref(props.block.action.target, props.projection.resolvedLinks) ?? '')
</script>

<template>
  <CallToAction
    v-if="block.heading && href"
    :eyebrow="block.eyebrow ?? undefined"
    :title="block.heading"
    :description="block.body ?? ''"
    :href="href"
    :label="block.action.label"
  />
</template>
