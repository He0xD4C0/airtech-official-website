<script setup lang="ts">
withDefaults(defineProps<{
  label: string
  tone?: 'neutral' | 'success' | 'warning' | 'danger' | 'info'
  dot?: boolean
}>(), {
  tone: 'neutral',
  dot: true,
})
</script>

<template>
  <span class="status-badge" :class="`status-badge--${tone}`">
    <i v-if="dot" aria-hidden="true"></i>{{ label }}
  </span>
</template>
