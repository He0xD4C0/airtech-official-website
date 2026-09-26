<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'

const props = defineProps<{ modelValue: boolean; title: string; labelledBy?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()
const dialog = ref<HTMLDialogElement | null>(null)
const titleId = props.labelledBy ?? `admin-dialog-${crypto.randomUUID()}`
let previousFocus: HTMLElement | null = null

function close(): void {
  dialog.value?.close()
  emit('update:modelValue', false)
}

function onCancel(event: Event): void {
  event.preventDefault()
  close()
}

function restoreFocus(): void {
  previousFocus?.focus()
  previousFocus = null
}

watch(() => props.modelValue, async (open) => {
  await nextTick()
  if (open && dialog.value && !dialog.value.open) {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    dialog.value.showModal()
  } else if (!open && dialog.value?.open) {
    dialog.value.close()
  }
}, { immediate: true })

onBeforeUnmount(() => {
  if (dialog.value?.open) dialog.value.close()
})
</script>

<template>
  <dialog ref="dialog" class="admin-dialog" :aria-labelledby="titleId" @cancel="onCancel" @close="restoreFocus">
    <form method="dialog" class="admin-dialog__card" @submit.prevent>
      <header><h2 :id="titleId">{{ title }}</h2></header>
      <div class="admin-dialog__body"><slot /></div>
      <footer><slot name="actions"><button class="button button--quiet" type="button" @click="close">关闭</button></slot></footer>
    </form>
  </dialog>
</template>

<style scoped>
@layer components {
.admin-dialog { width: min(34rem, calc(100% - 2rem)); padding: 0; border: 0; border-radius: 14px; color: var(--text-primary); box-shadow: 0 24px 60px rgb(11 38 48 / 28%); }
.admin-dialog::backdrop { background: rgb(11 38 48 / 48%); backdrop-filter: blur(2px); }
.admin-dialog__card { display: flex; flex-direction: column; gap: .8rem; padding: 1.1rem; }
.admin-dialog header, .admin-dialog footer { display: flex; align-items: center; justify-content: space-between; gap: .5rem; }
.admin-dialog h2, .admin-dialog p { margin: 0; }
.admin-dialog__body { display: flex; flex-direction: column; gap: .75rem; }
.admin-dialog footer { justify-content: flex-end; border-top: 1px solid var(--border-default); padding-top: .75rem; }
}
</style>
