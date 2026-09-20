<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import CookieBanner from './CookieBanner.vue'
import CompareTray from '@/components/product/CompareTray.vue'

const element = ref<HTMLElement>()
const height = ref(0)
let observer: ResizeObserver | undefined
onMounted(() => {
  if (!element.value || typeof ResizeObserver === 'undefined') return
  observer = new ResizeObserver(() => { height.value = element.value?.offsetHeight ?? 0 })
  observer.observe(element.value)
})
onUnmounted(() => observer?.disconnect())
</script>

<template>
  <div aria-hidden="true" :style="{ height: height ? `${height + 32}px` : '0' }" />
  <div ref="element" class="bottom-actions">
    <CompareTray />
    <CookieBanner />
  </div>
</template>

<style scoped>
.bottom-actions {
  position: fixed;
  z-index: 60;
  inset: auto 1rem 1rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: .75rem;
  max-height: calc(100dvh - 2rem);
  overflow-y: auto;
  pointer-events: none;
}
.bottom-actions :deep(aside) { position: static; width: 100%; flex-shrink: 0; pointer-events: auto; }
@media print { .bottom-actions { display: none; } }
</style>
