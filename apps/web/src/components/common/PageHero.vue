<script setup lang="ts">
import type { Breadcrumb } from '@/types/content'

defineProps<{
  eyebrow: string
  title: string
  description: string
  breadcrumbs?: Breadcrumb[]
  compact?: boolean
}>()
</script>

<template>
  <section :class="['page-hero', { compact }]">
    <div class="shell">
      <nav v-if="breadcrumbs?.length" class="breadcrumbs" aria-label="Breadcrumb">
        <ol>
          <li v-for="(item, index) in breadcrumbs" :key="`${item.label}-${index}`">
            <a v-if="item.href" :href="item.href">{{ item.label }}</a>
            <span v-else aria-current="page">{{ item.label }}</span>
          </li>
        </ol>
      </nav>
      <p class="eyebrow">{{ eyebrow }}</p>
      <h1>{{ title }}</h1>
      <p class="hero-description">{{ description }}</p>
      <slot />
    </div>
  </section>
</template>
