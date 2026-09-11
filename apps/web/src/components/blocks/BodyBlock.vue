<script setup lang="ts">
import { computed } from 'vue'
import StructuredContentRenderer from '@/components/content/StructuredContentRenderer'
import type { PublicContentProjection } from '@/types/projection'

const props = defineProps<{ projection: PublicContentProjection }>()

const document = computed(() => {
  const body = props.projection.body
  if (!body || body.content.length === 0) return undefined
  return body
})
</script>

<template>
  <section v-if="document" class="section shell published-content">
    <StructuredContentRenderer :document="document" />
  </section>
</template>
