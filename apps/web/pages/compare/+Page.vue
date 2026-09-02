<script setup lang="ts">
import { useData } from 'vike-vue/useData'
import PageRenderer from '@/components/pages/PageRenderer.vue'
import type { Data } from './+data'

const data = useData<Data>()
</script>

<template>
  <PageRenderer :page="data.page" />
</template>
