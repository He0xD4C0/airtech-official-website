<script setup lang="ts">
import { computed, ref } from 'vue'
import { AlertTriangle, Trash2 } from 'lucide-vue-next'
import type { ContentBlock, ContentRelationReference, RelationTargetReference } from '@airtek/contracts'
import { blockKindLabels } from '@/features/content/components/labels'
import BlockFieldsEditor from '@/features/content/components/blockInspectors/BlockFieldsEditor.vue'

const props = defineProps<{
  modelValue: ContentBlock
  relations: ContentRelationReference[]
}>()

const emit = defineEmits<{
  'update:modelValue': [value: ContentBlock]
  remove: []
  'add-relation': [target: RelationTargetReference, slot: string]
}>()

const confirmingRemove = ref(false)

const kindLabel = computed(() => blockKindLabels[props.modelValue.type] ?? props.modelValue.type)

const warnings = computed(() => {
  const block = props.modelValue
  const issues: string[] = []
  switch (block.type) {
    case 'hero':
      if (!block.heading && !block.media) issues.push('Hero 还没有标题或媒体。')
      break
    case 'featureGrid':
      if (!block.items.length) issues.push('特性网格还没有条目。')
      break
    case 'evidence':
      if (!block.items.length) issues.push('证据区块还没有条目。')
      break
    case 'cta':
      if (!block.heading.trim()) issues.push('CTA 缺少标题。')
      break
    case 'contactBlock':
      if (!block.channels.length) issues.push('联系块没有选择任何渠道。')
      break
    case 'downloadAsset':
      if (!block.label.trim()) issues.push('下载区块缺少链接文案。')
      break
    case 'relationCollection': {
      const linked = props.relations.some((relation) => (
        relation.slot === block.id || block.relationIds.includes(relation.id)
      ))
      if (!linked) issues.push('关联集合还没有关联任何实体。')
      break
    }
    default:
      break
  }
  return issues
})
</script>

<template>
  <div class="block-inspector">
    <header class="block-inspector__header">
      <div>
        <span>区块类型（不可切换）</span>
        <strong>{{ kindLabel }}</strong>
        <code :title="modelValue.id">ID {{ modelValue.id.slice(0, 8) }}…</code>
      </div>
      <button
        v-if="!confirmingRemove"
        class="icon-button"
        type="button"
        :aria-label="`删除${kindLabel}区块`"
        @click="confirmingRemove = true"
      >
        <Trash2 :size="16" />
      </button>
      <div v-else class="block-inspector__confirm" role="group" :aria-label="`确认删除${kindLabel}区块`">
        <button class="button button--quiet" type="button" @click="confirmingRemove = false">取消</button>
        <button class="button block-inspector__danger" type="button" @click="emit('remove')">确认删除</button>
      </div>
    </header>

    <ul v-if="warnings.length" class="block-inspector__warnings">
      <li v-for="warning in warnings" :key="warning"><AlertTriangle :size="13" />{{ warning }}</li>
    </ul>

    <BlockFieldsEditor
      :model-value="modelValue"
      :relations="relations"
      @update:model-value="emit('update:modelValue', $event)"
      @add-relation="(target, slot) => emit('add-relation', target, slot)"
    />
  </div>
</template>

<style scoped>
@layer components {
.block-inspector { display: flex; flex-direction: column; gap: 0.7rem; }
.block-inspector__header { display: flex; align-items: flex-start; justify-content: space-between; gap: 0.4rem; }
.block-inspector__header > div:first-child { display: flex; flex-direction: column; gap: 0.1rem; }
.block-inspector__header span { color: var(--text-secondary); font-size: 0.75rem; letter-spacing: 0.06em; text-transform: uppercase; }
.block-inspector__header strong { font-size: 0.75rem; }
.block-inspector__header code { color: var(--text-secondary); font-size: 0.75rem; }
.block-inspector__confirm { display: flex; gap: 0.25rem; }
.block-inspector__warnings { display: flex; flex-direction: column; gap: 0.25rem; margin: 0; padding: 0.45rem 0.55rem; border: 1px solid #f0dca8; border-radius: 8px; background: var(--surface-warning); list-style: none; }
.block-inspector__warnings li { display: flex; align-items: center; gap: 0.3rem; color: #8a6100; font-size: 0.75rem; }
.button.block-inspector__danger { border-color: #d64545; background: #d64545; color: white; }
.button.block-inspector__danger:hover:not(:disabled) { border-color: #b33535; background: #b33535; }
}
</style>
