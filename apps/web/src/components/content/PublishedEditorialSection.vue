<script setup lang="ts">
import { computed } from 'vue'
import type { ContentEntry } from '@airtek/contracts'
import StructuredContentRenderer from './StructuredContentRenderer'
import { hasRenderableRichText } from '@/lib/richText'

const props = defineProps<{ content?: ContentEntry }>()
const visible = computed(() => (
  props.content?.status === 'published'
  && hasRenderableRichText(props.content.body)
))
</script>

<template>
  <section v-if="visible && content" class="section shell published-editorial" aria-label="Published editorial content">
    <StructuredContentRenderer :document="content.body" />
  </section>
</template>
