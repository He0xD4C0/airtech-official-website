<script setup lang="ts">
import { computed } from 'vue'
import type { CmsBodyPolicy, ContentDraftV2 } from '@airtek/contracts'
import StructuredEditor from '@/components/StructuredEditor.vue'
import { bodyPolicyLabel } from '@/services/contentTemplates'

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
  emit('update:modelValue', value as BodyDocument)
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
.structured-body { display: flex; flex-direction: column; gap: 0.4rem; }
.structured-body__policy { margin: 0; color: var(--admin-muted); font-size: 0.58rem; }
.structured-body__blocked { margin: 0; padding: 0.65rem; border: 1px dashed var(--admin-line); border-radius: 9px; color: var(--admin-muted); font-size: 0.6rem; }
</style>
