<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vike-vue/useData'
import { canonicalUrl, openGraphType as resolveOpenGraphType, robotsDirective, shouldEmitStructuredData } from '@/shared/lib/seo'
import { buildPublicStructuredData } from '@/app/lib/structuredData'
import type { Data } from './+data'

const data = useData<Data>()
const origin = data.publicOrigin
const canonical = computed(() => canonicalUrl(origin, data.page))
const robots = computed(() => robotsDirective(data.page))
const openGraphType = computed(() => resolveOpenGraphType(data.page))
const hasStructuredData = computed(() => shouldEmitStructuredData(data.page))
const schema = computed(() => JSON.stringify(buildPublicStructuredData(data.page, origin, data.site)).replace(/</g, '\\u003c'))
</script>

<template>
  <link rel="canonical" :href="canonical">
  <meta name="robots" :content="robots">
  <meta property="og:type" :content="openGraphType">
  <meta property="og:site_name" :content="data.site.brandName">
  <meta property="og:url" :content="canonical">
  <meta property="og:locale" content="en_US">
  <meta property="og:title" :content="data.page.metaTitle">
  <meta v-if="data.page.description" property="og:description" :content="data.page.description">
  <meta name="twitter:card" content="summary">
  <meta name="twitter:title" :content="data.page.metaTitle">
  <meta v-if="data.page.description" name="twitter:description" :content="data.page.description">
  <script v-if="hasStructuredData" type="application/ld+json" v-html="schema" />
</template>
