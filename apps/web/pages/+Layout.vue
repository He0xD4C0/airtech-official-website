<script setup lang="ts">
import '@/app/styles/site.css'
import SiteShell from '@/app/layout/SiteShell.vue'
</script>

<template>
  <SiteShell>
    <slot />
  </SiteShell>
</template>
