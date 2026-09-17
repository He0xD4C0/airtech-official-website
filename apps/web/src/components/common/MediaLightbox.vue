<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from 'vue'

defineProps<{
  previewSrc: string
  originalSrc: string
  alt: string
  caption?: string | null
}>()

const dialog = ref<HTMLDialogElement | null>(null)
const open = ref(false)
const loaded = ref(false)
let previousFocus: HTMLElement | null = null

async function show(): Promise<void> {
  previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  loaded.value = false
  open.value = true
  await nextTick()
  dialog.value?.showModal()
}

function close(): void {
  dialog.value?.close()
}

function afterClose(): void {
  open.value = false
  previousFocus?.focus()
  previousFocus = null
}

onBeforeUnmount(() => {
  if (dialog.value?.open) dialog.value.close()
})
</script>

<template>
  <figure class="media-lightbox">
    <button class="media-lightbox__trigger" type="button" aria-label="View original image" @click="show">
      <img :src="previewSrc" :alt="alt" loading="lazy" decoding="async" />
    </button>
    <figcaption v-if="caption">{{ caption }}</figcaption>
    <dialog ref="dialog" class="media-lightbox__dialog" aria-label="Original image viewer" @close="afterClose">
      <button class="media-lightbox__close" type="button" aria-label="Close original image" @click="close">Close</button>
      <p v-if="open && !loaded" class="media-lightbox__loading" role="status">Loading original image…</p>
      <img v-if="open" :src="originalSrc" :alt="alt" @load="loaded = true" />
    </dialog>
  </figure>
</template>

<style scoped>
@layer components {
  .media-lightbox { margin: 0; }
  .media-lightbox__trigger { display: block; width: 100%; padding: 0; border: 0; background: transparent; cursor: zoom-in; }
  .media-lightbox__trigger img { display: block; width: 100%; height: auto; }
  .media-lightbox__dialog { max-width: min(92vw, 90rem); max-height: 92vh; padding: 3rem 1rem 1rem; border: 0; border-radius: var(--airtek-radius-lg); background: var(--airtek-ink); color: white; }
  .media-lightbox__dialog::backdrop { background: rgb(0 0 0 / 75%); }
  .media-lightbox__dialog img { display: block; max-width: 100%; max-height: calc(92vh - 4rem); margin-inline: auto; object-fit: contain; }
  .media-lightbox__close { position: absolute; inset-block-start: 0.5rem; inset-inline-end: 0.5rem; min-width: 2.75rem; min-height: 2.75rem; border: 1px solid rgb(255 255 255 / 40%); border-radius: 999px; background: transparent; color: white; }
  .media-lightbox__loading { text-align: center; }
}
</style>
