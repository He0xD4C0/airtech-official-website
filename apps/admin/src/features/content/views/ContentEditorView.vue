<script setup lang="ts">
import { onBeforeUnmount, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { onBeforeRouteLeave } from 'vue-router'
import ContentEditorShell from '@/features/content/components/ContentEditorShell.vue'
import { useContentEditorStore } from '@/features/content/stores/contentEditor'

const route = useRoute()
const store = useContentEditorStore()

async function load(): Promise<void> {
  const id = String(route.params.draftId ?? '')
  if (!id) return
  store.reset()
  await store.load(id)
}

onMounted(load)
watch(
  () => route.params.draftId,
  () => {
    if (route.name === 'content-draft-editor') void load()
  },
)

function confirmUnsaved(): boolean {
  return !store.isDirty || window.confirm('存在未保存修改，确定离开吗？')
}

function beforeUnload(event: BeforeUnloadEvent): void {
  if (!store.isDirty) return
  event.preventDefault()
  event.returnValue = ''
}

function onEditorKeydown(event: KeyboardEvent): void {
  if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== 'z') return
  event.preventDefault()
  if (event.shiftKey) store.redo()
  else store.undo()
}

onBeforeRouteLeave(() => confirmUnsaved())
onMounted(() => {
  window.addEventListener('beforeunload', beforeUnload)
  window.addEventListener('keydown', onEditorKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('beforeunload', beforeUnload)
  window.removeEventListener('keydown', onEditorKeydown)
  store.reset()
})
</script>

<template>
  <ContentEditorShell @reload="load" />
</template>
