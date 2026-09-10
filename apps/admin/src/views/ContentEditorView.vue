<script setup lang="ts">
import { onBeforeUnmount, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import ContentEditorShell from '@/components/content/ContentEditorShell.vue'
import { useContentEditorStore } from '@/stores/contentEditor'

const route = useRoute()
const store = useContentEditorStore()

async function load(): Promise<void> {
  const id = String(route.params.id ?? '')
  if (!id) return
  store.reset()
  await store.load(id)
}

onMounted(load)
watch(
  () => route.params.id,
  () => {
    if (route.name === 'content-editor') void load()
  },
)
onBeforeUnmount(() => store.reset())
</script>

<template>
  <ContentEditorShell @reload="load" />
</template>
