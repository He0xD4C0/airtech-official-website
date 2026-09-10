<script setup lang="ts">
import { MousePointerClick, Plus, Trash2 } from 'lucide-vue-next'
import type { EditorialAction, LinkTargetReference } from '@airtek/contracts'
import LinkTargetField from './LinkTargetField.vue'

const props = defineProps<{
  modelValue: EditorialAction | null
  label?: string
}>()

const emit = defineEmits<{ 'update:modelValue': [value: EditorialAction | null] }>()

function create(): void {
  emit('update:modelValue', { label: '', target: { targetType: 'route', path: '' } })
}

function updateLabel(value: string): void {
  if (!props.modelValue) return
  emit('update:modelValue', { ...props.modelValue, label: value })
}

function updateTarget(target: LinkTargetReference): void {
  if (!props.modelValue) return
  emit('update:modelValue', { ...props.modelValue, target })
}

function clear(): void {
  emit('update:modelValue', null)
}
</script>

<template>
  <div class="action-field">
    <div v-if="!modelValue" class="action-field__empty">
      <MousePointerClick :size="15" />
      <span>{{ label ?? '动作' }}尚未设置</span>
      <button class="button button--quiet" type="button" @click="create"><Plus :size="14" />添加动作</button>
    </div>
    <template v-else>
      <div class="action-field__header">
        <strong>{{ label ?? '动作' }}</strong>
        <button class="icon-button" type="button" aria-label="移除动作" @click="clear"><Trash2 :size="15" /></button>
      </div>
      <label class="field">
        <span>按钮文案</span>
        <input
          :value="modelValue.label"
          maxlength="120"
          placeholder="例如 Request a quote"
          @input="updateLabel(($event.target as HTMLInputElement).value)"
        />
      </label>
      <LinkTargetField :model-value="modelValue.target" @update:model-value="updateTarget" />
    </template>
  </div>
</template>

<style scoped>
.action-field { display: flex; flex-direction: column; gap: 0.5rem; }
.action-field__empty { display: flex; align-items: center; gap: 0.45rem; padding: 0.55rem; border: 1px dashed var(--admin-line); border-radius: 8px; color: var(--admin-muted); font-size: 0.62rem; }
.action-field__empty span { flex: 1; }
.action-field__header { display: flex; align-items: center; justify-content: space-between; }
.action-field__header strong { font-size: 0.64rem; }
</style>
