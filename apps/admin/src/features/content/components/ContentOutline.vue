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
  if (!next) return
  const list = (event.currentTarget as HTMLElement).closest('ol')
  const buttons = list ? [...list.querySelectorAll<HTMLButtonElement>('button')] : []
  buttons[index + offset]?.focus()
  emit('select', next.id)
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
@layer components {
  .content-outline { display: flex; min-width: 0; flex-direction: column; gap: var(--space-2); align-self: start; }
  .content-outline__title { margin: 0; color: var(--text-secondary); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; }
  .content-outline__list { display: flex; flex-direction: column; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
  .content-outline__item { display: flex; align-items: center; gap: var(--space-1); width: 100%; min-height: 2.75rem; padding: var(--space-2); border: 1px solid transparent; border-radius: 0.5rem; background: transparent; color: var(--text-primary); font-size: 0.75rem; text-align: start; }
  .content-outline__item svg { color: var(--text-secondary); }
  .content-outline__item.is-nested { padding-inline-start: var(--space-4); color: var(--text-secondary); }
  .content-outline__item:hover { background: var(--surface-info); }
  .content-outline__item[aria-current='true'] { border-color: var(--airtek-blue); background: var(--surface-info); color: var(--airtek-blue-dark); font-weight: 600; }
  .content-outline__hint { margin: 0; color: var(--text-secondary); font-size: 0.75rem; line-height: 1.45; }

  @container content-editor (min-width: 52rem) and (max-width: 71.999rem) {
    .content-outline__list { flex-flow: row wrap; }
    .content-outline__item { width: auto; }
    .content-outline__item.is-nested { padding-inline-start: var(--space-2); }
  }

  @container content-editor (min-width: 72rem) {
    .content-outline { position: sticky; top: var(--space-3); min-width: 11rem; }
  }
}
</style>
