<script setup lang="ts">
import { ArrowDown, ArrowUp, CornerDownRight, Plus, Trash2 } from 'lucide-vue-next'
import type { LinkTargetReference, NavigationItem } from '@airtek/contracts'
import { newDraftId } from '@/features/content/services/contentDraftDefaults'
import LinkTargetField from '@/features/content/components/fields/LinkTargetField.vue'

defineOptions({ name: 'NavigationItemEditor' })

const props = withDefaults(defineProps<{
  modelValue: NavigationItem
  depth?: number
  index?: number
  count?: number
}>(), {
  depth: 0,
  index: 0,
  count: 1,
})

const emit = defineEmits<{
  'update:modelValue': [value: NavigationItem]
  remove: []
  move: [direction: 'up' | 'down']
}>()

function updateLabel(value: string): void {
  emit('update:modelValue', { ...props.modelValue, label: value })
}

function updateTarget(target: LinkTargetReference | null): void {
  emit('update:modelValue', { ...props.modelValue, target })
}

function toggleTarget(enabled: boolean): void {
  updateTarget(enabled ? { targetType: 'route', path: '' } : null)
}

function updateChild(index: number, child: NavigationItem): void {
  const children = [...props.modelValue.children]
  children[index] = child
  emit('update:modelValue', { ...props.modelValue, children })
}

function removeChild(index: number): void {
  emit('update:modelValue', {
    ...props.modelValue,
    children: props.modelValue.children.filter((_, position) => position !== index),
  })
}

function moveChild(index: number, direction: 'up' | 'down'): void {
  const target = direction === 'up' ? index - 1 : index + 1
  const children = [...props.modelValue.children]
  if (target < 0 || target >= children.length) return
  const [entry] = children.splice(index, 1)
  children.splice(target, 0, entry)
  emit('update:modelValue', { ...props.modelValue, children })
}

function addChild(): void {
  emit('update:modelValue', {
    ...props.modelValue,
    children: [...props.modelValue.children, { id: newDraftId(), label: '', target: null, children: [] }],
  })
}
</script>

<template>
  <li class="nav-item" :style="{ '--nav-depth': String(depth) }">
    <div class="nav-item__card">
      <div class="nav-item__header">
        <CornerDownRight v-if="depth > 0" :size="14" aria-hidden="true" />
        <strong>层级 {{ depth + 1 }} · 导航项 {{ index + 1 }}</strong>
        <div class="nav-item__controls">
          <button
            class="icon-button"
            type="button"
            :disabled="index === 0"
            :aria-label="`上移导航项 ${index + 1}`"
            @click="emit('move', 'up')"
          >
            <ArrowUp :size="14" />
          </button>
          <button
            class="icon-button"
            type="button"
            :disabled="index >= count - 1"
            :aria-label="`下移导航项 ${index + 1}`"
            @click="emit('move', 'down')"
          >
            <ArrowDown :size="14" />
          </button>
          <button class="icon-button" type="button" :aria-label="`删除导航项 ${index + 1}`" @click="emit('remove')">
            <Trash2 :size="14" />
          </button>
        </div>
      </div>

      <label class="field">
        <span>显示名称</span>
        <input
          :value="modelValue.label"
          maxlength="120"
          placeholder="例如 Products"
          @input="updateLabel(($event.target as HTMLInputElement).value)"
        />
      </label>

      <label class="toggle-row">
        <span><strong>设置链接</strong><small>不设置时该项仅作为分组标题</small></span>
        <input
          type="checkbox"
          :checked="Boolean(modelValue.target)"
          @change="toggleTarget(($event.target as HTMLInputElement).checked)"
        />
      </label>
      <LinkTargetField
        v-if="modelValue.target"
        :model-value="modelValue.target"
        @update:model-value="updateTarget"
      />

      <ul v-if="modelValue.children.length" class="nav-item__children">
        <NavigationItemEditor
          v-for="(child, childIndex) in modelValue.children"
          :key="child.id"
          :model-value="child"
          :depth="depth + 1"
          :index="childIndex"
          :count="modelValue.children.length"
          @update:model-value="updateChild(childIndex, $event)"
          @remove="removeChild(childIndex)"
          @move="moveChild(childIndex, $event)"
        />
      </ul>

      <button v-if="depth < 2" class="button button--quiet" type="button" @click="addChild">
        <Plus :size="14" />添加子项
      </button>
    </div>
  </li>
</template>

<style scoped>
@layer components {
.nav-item { margin: 0; list-style: none; }
.nav-item__card { display: flex; flex-direction: column; gap: 0.5rem; padding: 0.6rem; margin-top: 0.5rem; border: 1px solid var(--border-default); border-radius: 9px; background: white; }
.nav-item__header { display: flex; align-items: center; gap: 0.35rem; color: var(--text-secondary); }
.nav-item__header strong { flex: 1; font-size: 0.75rem; letter-spacing: 0.04em; text-transform: uppercase; }
.nav-item__controls { display: flex; align-items: center; gap: 0.15rem; }
.nav-item__children { padding: 0 0 0 0.6rem; margin: 0; border-left: 2px solid var(--border-subtle); }
}
</style>
