<script setup lang="ts">
import { buildTimePublicOrigin } from '@/lib/publicOrigin'

const origin = buildTimePublicOrigin()
</script>

<template>
  <meta name="robots" content="noindex,follow,noarchive">
  <link rel="canonical" :href="`${origin}/en/products/compare`">
</template>
