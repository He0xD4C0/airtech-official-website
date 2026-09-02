<script setup lang="ts">
import type { PublicPageSection } from '@/types/content'

defineProps<{ sections?: PublicPageSection[] }>()
</script>

<template>
  <section
    v-for="section in sections"
    :id="section.id"
    :key="section.id"
    class="section shell page-slot-section"
  >
    <p v-if="section.eyebrow" class="eyebrow">{{ section.eyebrow }}</p>
    <h2 v-if="section.title">{{ section.title }}</h2>
    <p v-if="section.description">{{ section.description }}</p>
    <div v-if="section.links?.length" class="button-row">
      <a v-for="link in section.links" :key="`${section.id}-${link.href}`" class="button secondary" :href="link.href">{{ link.label }}</a>
    </div>
  </section>
</template>
