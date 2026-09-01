<script setup lang="ts">
import type { Component } from 'vue'

withDefaults(defineProps<{
  label: string
  value: string
  detail: string
  icon: Component
  tone?: 'blue' | 'green' | 'amber' | 'slate'
}>(), {
  tone: 'blue',
})
</script>

<template>
  <article class="metric-card">
    <div class="metric-card__icon" :class="`metric-card__icon--${tone}`"><component :is="icon" :size="19" /></div>
    <div>
      <p>{{ label }}</p>
      <strong>{{ value }}</strong>
      <span>{{ detail }}</span>
    </div>
  </article>
</template>
