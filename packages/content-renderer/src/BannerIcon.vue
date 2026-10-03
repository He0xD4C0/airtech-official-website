<script setup lang="ts">
defineProps<{ kind: 'previous' | 'next' | 'play' | 'pause' }>()
const paths = {
  previous: 'M15 5 8 12l7 7',
  next: 'm9 5 7 7-7 7',
  play: 'm8 5 11 7-11 7Z',
  pause: 'M8 5v14M16 5v14',
}
</script>

<template>
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
    <path :d="paths[kind]" />
  </svg>
</template>
