<script setup lang="ts">
import { computed } from 'vue'
import { Lock, ShieldAlert } from 'lucide-vue-next'
import type { CmsContentKind, ContentTypeFields, FaqItem, FooterTypeFields, GeneralInformationTypeFields, NavigationTypeFields } from '@airtek/contracts'
import EditorialFields from '@/features/content/components/typeFields/EditorialFields.vue'
import FaqItemsEditor from '@/features/content/components/typeFields/FaqItemsEditor.vue'
import FooterFields from '@/features/content/components/typeFields/FooterFields.vue'
import GeneralInformationFields from '@/features/content/components/typeFields/GeneralInformationFields.vue'
import NavigationFields from '@/features/content/components/typeFields/NavigationFields.vue'
import { contentKindLabels } from '@/features/content/components/labels'

const props = withDefaults(defineProps<{
  modelValue: ContentTypeFields
  kind?: CmsContentKind | null
}>(), {
  kind: null,
})

const emit = defineEmits<{ 'update:modelValue': [value: ContentTypeFields] }>()

const kindMismatch = computed(() => Boolean(props.kind && props.kind !== props.modelValue.type))
const lockedLabel = computed(() => {
  const labels = contentKindLabels as Record<string, string | undefined>
  return labels[props.modelValue.type] ?? props.modelValue.type
})

const faqItems = computed(() => (props.modelValue.type === 'faq' ? props.modelValue.items : null))

function updateFaqItems(items: FaqItem[]): void {
  if (props.modelValue.type !== 'faq') return
  emit('update:modelValue', { ...props.modelValue, items })
}

function updateGeneralInformation(value: GeneralInformationTypeFields): void {
  emit('update:modelValue', { ...value, type: 'generalInformation' })
}

function updateNavigation(value: NavigationTypeFields): void {
  emit('update:modelValue', { ...value, type: 'navigation' })
}

function updateFooter(value: FooterTypeFields): void {
  emit('update:modelValue', { ...value, type: 'footer' })
}
</script>

<template>
  <div class="type-panel">
    <p v-if="kindMismatch" class="type-panel__guard" role="alert">
      <ShieldAlert :size="16" />
      <span>
        类型字段（{{ modelValue.type }}）与模板锁定的内容类型（{{ kind }}）不一致。
        为避免写入不一致数据，这里已停止结构编辑；请重新载入内容或联系管理员修复数据。
      </span>
    </p>
    <template v-else>
      <p class="type-panel__lock">
        <Lock :size="14" />
        <span>内容类型锁定为 {{ lockedLabel }}；类型字段随模板固定，编辑器不提供切换入口。</span>
      </p>
      <GeneralInformationFields
        v-if="modelValue.type === 'generalInformation'"
        :model-value="modelValue"
        @update:model-value="updateGeneralInformation"
      />
      <NavigationFields
        v-else-if="modelValue.type === 'navigation'"
        :model-value="modelValue"
        @update:model-value="updateNavigation"
      />
      <FooterFields
        v-else-if="modelValue.type === 'footer'"
        :model-value="modelValue"
        @update:model-value="updateFooter"
      />
      <FaqItemsEditor
        v-else-if="faqItems"
        :model-value="faqItems"
        @update:model-value="updateFaqItems"
      />
      <EditorialFields
        v-else
        :model-value="modelValue"
        @update:model-value="emit('update:modelValue', $event)"
      />
    </template>
  </div>
</template>

<style scoped>
@layer components {
.type-panel { display: flex; flex-direction: column; gap: 0.85rem; }
.type-panel__lock { display: flex; align-items: flex-start; gap: 0.4rem; margin: 0; color: var(--text-secondary); font-size: 0.75rem; line-height: 1.5; }
.type-panel__guard { display: flex; align-items: flex-start; gap: 0.45rem; padding: 0.7rem; margin: 0; border: 1px solid #f2c9c2; border-radius: 9px; background: var(--surface-danger); color: #8a2c1c; font-size: 0.75rem; line-height: 1.55; }
}
</style>
