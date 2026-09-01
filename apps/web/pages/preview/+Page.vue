<script setup lang="ts">
import { useData } from 'vike-vue/useData'
import ContentPreviewPage from '@/components/pages/ContentPreviewPage.vue'
import type { Data } from './+data'

const data = useData<Data>()
</script>

<template>
  <ContentPreviewPage :preview="data" />
</template>
