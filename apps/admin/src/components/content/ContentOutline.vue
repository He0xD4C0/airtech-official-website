<script setup lang="ts">
import { ChevronRight } from 'lucide-vue-next'

export interface OutlineSection {
  id: string
  label: string
  level?: 1 | 2
}

const props = defineProps<{
  sections: OutlineSection[]
  activeId: string
}>()

const emit = defineEmits<{ select: [id: string] }>()

function onKeydown(event: KeyboardEvent, index: number): void {
  if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
  event.preventDefault()
  const offset = event.key === 'ArrowDown' ? 1 : -1
  const next = props.sections[index + offset]
  if (next) emit('select', next.id)
}
</script>

<template>
  <nav class="content-outline" aria-label="编辑器大纲">
    <h2 class="content-outline__title">大纲</h2>
    <ol class="content-outline__list">
      <li v-for="(section, index) in sections" :key="section.id">
        <button
          type="button"
          class="content-outline__item"
          :class="{ 'is-active': section.id === activeId, 'is-nested': section.level === 1 }"
          :aria-current="section.id === activeId ? 'true' : undefined"
          @click="emit('select', section.id)"
          @keydown="onKeydown($event, index)"
        >
          <ChevronRight :size="13" aria-hidden="true" />
          <span>{{ section.label }}</span>
        </button>
      </li>
    </ol>
    <p class="content-outline__hint">使用 ↑ / ↓ 在大纲项之间移动，Enter 跳转。</p>
  </nav>
</template>

<style scoped>
.content-outline { position: sticky; top: 0.75rem; display: flex; flex-direction: column; gap: 0.35rem; align-self: start; min-width: 11rem; }
.content-outline__title { margin: 0 0 0.15rem; color: var(--admin-muted); font-size: 0.6rem; text-transform: uppercase; letter-spacing: 0.06em; }
.content-outline__list { display: flex; flex-direction: column; gap: 0.2rem; margin: 0; padding: 0; list-style: none; }
.content-outline__item { display: flex; align-items: center; gap: 0.3rem; width: 100%; padding: 0.4rem 0.5rem; border: 1px solid transparent; border-radius: 8px; background: transparent; color: var(--admin-ink); font-size: 0.66rem; text-align: left; }
.content-outline__item svg { color: var(--admin-muted); }
.content-outline__item.is-nested { padding-left: 1.05rem; color: var(--admin-muted); font-size: 0.63rem; }
.content-outline__item:hover { background: var(--admin-soft-blue); }
.content-outline__item.is-active { border-color: var(--airtek-blue); background: var(--admin-soft-blue); color: var(--airtek-blue-dark); font-weight: 600; }
.content-outline__item:focus-visible { outline: 2px solid var(--airtek-blue); outline-offset: 1px; }
.content-outline__hint { margin: 0.35rem 0 0; color: var(--admin-muted); font-size: 0.55rem; line-height: 1.45; }
</style>
