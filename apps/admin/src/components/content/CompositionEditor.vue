<script setup lang="ts">
import { computed } from 'vue'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { ContentBlock, ContentBlockKind, ContentTemplateDefinition } from '@airtek/contracts'
import { blockKindLabels } from '@/components/content/labels'
import { addableBlockKinds, isRequiredBlock, moveBlock } from './compositionOrdering'

const props = defineProps<{
  modelValue: ContentBlock[]
  template: ContentTemplateDefinition
  selectedBlockId: string | null
}>()

const emit = defineEmits<{
  'update:modelValue': [value: ContentBlock[]]
  select: [id: string]
  'add-block': [kind: ContentBlockKind]
  'remove-block': [id: string]
}>()

const requiredKinds = computed(() => new Set<ContentBlockKind>(props.template.requiredBlocks))
const addableKinds = computed(() =>
  addableBlockKinds(props.template.allowedBlocks, props.template.requiredBlocks),
)
const usesBlocks = computed(() => props.template.allowedBlocks.length > 0)

function isRequired(block: ContentBlock): boolean {
  return isRequiredBlock(requiredKinds.value, block)
}

function canMove(block: ContentBlock, offset: -1 | 1): boolean {
  return moveBlock(props.modelValue, props.template.requiredBlocks, block.id, offset) !== null
}

function move(block: ContentBlock, offset: -1 | 1): void {
  const next = moveBlock(props.modelValue, props.template.requiredBlocks, block.id, offset)
  if (next) emit('update:modelValue', next)
}

function onSelectKeydown(event: KeyboardEvent, block: ContentBlock): void {
  if (!event.altKey) return
  if (event.key === 'ArrowUp') {
    event.preventDefault()
    move(block, -1)
  } else if (event.key === 'ArrowDown') {
    event.preventDefault()
    move(block, 1)
  }
}

function text(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim()
  return trimmed ? trimmed : null
}

function summary(block: ContentBlock): string {
  switch (block.type) {
    case 'hero':
      return text(block.heading) ?? text(block.eyebrow) ?? '未填写标题'
    case 'body':
      return 'Tiptap 正文文档'
    case 'media':
      return text(block.caption) ?? text(block.media.altText) ?? '媒体区块'
    case 'featureGrid':
      return text(block.heading) ?? `${block.items.length} 个特性`
    case 'evidence':
      return text(block.heading) ?? `${block.items.length} 条证据`
    case 'cta':
      return text(block.heading) || '行动按钮'
    case 'relationCollection':
      return text(block.heading) ?? `${block.relationIds.length} 条关联`
    case 'faqCollection':
      return text(block.heading) ?? 'FAQ 集合'
    case 'downloadAsset':
      return text(block.label) ?? '下载资源'
    case 'contactBlock':
      return text(block.heading) ?? '联系信息'
  }
}
</script>

<template>
  <div class="composition-editor">
    <p v-if="usesBlocks" class="composition-editor__hint">
      必需区块由受控模板固定；可选区块可添加、移除与排序（Alt + ↑ / ↓ 移动）。
    </p>

    <ol v-if="modelValue.length" class="composition-editor__list">
      <li
        v-for="(block, index) in modelValue"
        :key="block.id"
        class="composition-editor__item"
        :class="{ 'is-selected': block.id === selectedBlockId, 'is-required': isRequired(block) }"
      >
        <button
          type="button"
          class="composition-editor__select"
          :aria-current="block.id === selectedBlockId ? 'true' : undefined"
          @click="emit('select', block.id)"
          @keydown="onSelectKeydown($event, block)"
        >
          <span class="composition-editor__index" aria-hidden="true">{{ index + 1 }}</span>
          <span class="composition-editor__meta">
            <strong>{{ blockKindLabels[block.type] }}</strong>
            <small>{{ summary(block) }}</small>
          </span>
          <span v-if="isRequired(block)" class="composition-editor__badge">必需</span>
        </button>
        <div class="composition-editor__actions">
          <button
            type="button"
            class="icon-button"
            :aria-label="`上移${blockKindLabels[block.type]}区块`"
            :disabled="!canMove(block, -1)"
            @click="move(block, -1)"
          >
            <ArrowUp :size="14" />
          </button>
          <button
            type="button"
            class="icon-button"
            :aria-label="`下移${blockKindLabels[block.type]}区块`"
            :disabled="!canMove(block, 1)"
            @click="move(block, 1)"
          >
            <ArrowDown :size="14" />
          </button>
          <button
            type="button"
            class="icon-button"
            :aria-label="`移除${blockKindLabels[block.type]}区块`"
            :disabled="isRequired(block)"
            @click="emit('remove-block', block.id)"
          >
            <Trash2 :size="14" />
          </button>
        </div>
      </li>
    </ol>

    <p v-else class="composition-editor__empty">
      {{ usesBlocks ? '尚未添加区块，可从下方添加可选区块。' : '该模板不使用页面组成区块，内容由类型字段管理。' }}
    </p>

    <div v-if="addableKinds.length" class="composition-editor__add" role="group" aria-label="添加可选区块">
      <button
        v-for="kind in addableKinds"
        :key="kind"
        type="button"
        class="button button--quiet"
        :aria-label="`添加${blockKindLabels[kind]}区块`"
        @click="emit('add-block', kind)"
      >
        <Plus :size="14" />{{ blockKindLabels[kind] }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.composition-editor { display: flex; flex-direction: column; gap: 0.5rem; }
.composition-editor__hint { margin: 0; color: var(--admin-muted); font-size: 0.58rem; }
.composition-editor__list { display: flex; flex-direction: column; gap: 0.35rem; margin: 0; padding: 0; list-style: none; }
.composition-editor__item { display: flex; align-items: stretch; gap: 0.3rem; border: 1px solid var(--admin-line); border-radius: 9px; background: white; }
.composition-editor__item.is-selected { border-color: var(--airtek-blue); box-shadow: 0 0 0 1px var(--airtek-blue); }
.composition-editor__select { display: flex; flex: 1; align-items: center; gap: 0.45rem; min-width: 0; padding: 0.45rem 0.5rem; border: 0; border-radius: 9px 0 0 9px; background: transparent; text-align: left; }
.composition-editor__select:focus-visible { outline: 2px solid var(--airtek-blue); outline-offset: -2px; }
.composition-editor__index { display: grid; place-items: center; width: 1.35rem; height: 1.35rem; border-radius: 6px; background: var(--admin-soft-blue); color: var(--airtek-blue-dark); font-size: 0.58rem; }
.composition-editor__meta { display: flex; flex-direction: column; min-width: 0; }
.composition-editor__meta strong { font-size: 0.68rem; }
.composition-editor__meta small { color: var(--admin-muted); font-size: 0.58rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.composition-editor__badge { padding: 0.08rem 0.3rem; border-radius: 5px; background: var(--admin-soft-blue); color: var(--airtek-blue-dark); font-size: 0.52rem; }
.composition-editor__actions { display: flex; align-items: center; gap: 0.1rem; padding: 0 0.35rem; border-left: 1px solid var(--admin-line-soft); }
.composition-editor__actions .icon-button:disabled { opacity: 0.35; cursor: not-allowed; }
.composition-editor__empty { margin: 0; padding: 0.65rem; border: 1px dashed var(--admin-line); border-radius: 9px; color: var(--admin-muted); font-size: 0.6rem; }
.composition-editor__add { display: flex; flex-wrap: wrap; gap: 0.3rem; }
</style>
