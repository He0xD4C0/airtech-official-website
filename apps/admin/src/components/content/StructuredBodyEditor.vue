<script setup lang="ts">
import { computed } from 'vue'
import type { CmsBodyPolicy, ContentDraftV2 } from '@airtek/contracts'
import StructuredEditor from '@/components/StructuredEditor.vue'
import { bodyPolicyLabel } from '@/components/content/labels'

type BodyDocument = NonNullable<ContentDraftV2['body']>

const props = defineProps<{
  modelValue: BodyDocument | null | undefined
  policy: CmsBodyPolicy
}>()

const emit = defineEmits<{ 'update:modelValue': [value: BodyDocument] }>()

function emptyDocument(): BodyDocument {
  return { type: 'doc', content: [] } as BodyDocument
}

const document = computed<BodyDocument>(() => props.modelValue ?? emptyDocument())

function onUpdate(value: unknown): void {
  const document = (typeof value === 'object' && value !== null ? value : {}) as {
    content?: unknown
  }
  emit('update:modelValue', {
    type: 'doc',
    content: Array.isArray(document.content) ? document.content : [],
  } as BodyDocument)
}
</script>

<template>
  <div class="structured-body">
    <p class="structured-body__policy">正文策略：{{ bodyPolicyLabel(policy) }}</p>
    <p v-if="policy === 'forbidden'" class="structured-body__blocked">
      受控模板禁用了 Tiptap 正文，内容结构由类型字段与区块管理。
    </p>
    <StructuredEditor v-else :model-value="document" @update:model-value="onUpdate" />
  </div>
</template>

<style scoped>
@layer components {
.structured-body { display: flex; flex-direction: column; gap: 0.4rem; }
.structured-body__policy { margin: 0; color: var(--text-secondary); font-size: 0.75rem; }
.structured-body__blocked { margin: 0; padding: 0.65rem; border: 1px dashed var(--border-default); border-radius: 9px; color: var(--text-secondary); font-size: 0.75rem; }
}
</style>
