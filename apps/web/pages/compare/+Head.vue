<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vike-vue/useData'
import { canonicalUrl, robotsDirective } from '@/shared/lib/seo'
import type { Data } from './+data'

const data = useData<Data>()
const canonical = computed(() => canonicalUrl(data.publicOrigin, data.page))
</script>

<template>
  <meta name="robots" :content="robotsDirective(data.page)">
  <link rel="canonical" :href="canonical">
  <meta property="og:site_name" :content="data.site.brandName">
  <meta property="og:title" :content="data.page.metaTitle">
  <meta v-if="data.page.description" property="og:description" :content="data.page.description">
</template>
