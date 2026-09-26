<script setup lang="ts">
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { NavigationItem, NavigationTypeFields } from '@airtek/contracts'
import { newDraftId } from '@/features/content/services/contentDraftDefaults'
import NavigationItemEditor from '@/features/content/components/fields/NavigationItemEditor.vue'

const props = defineProps<{ modelValue: NavigationTypeFields }>()
const emit = defineEmits<{ 'update:modelValue': [value: NavigationTypeFields] }>()

function setItems(items: NavigationItem[]): void {
  emit('update:modelValue', { ...props.modelValue, items })
}

function updateItem(index: number, item: NavigationItem): void {
  setItems(props.modelValue.items.map((entry, position) => (position === index ? item : entry)))
}

function removeItem(index: number): void {
  setItems(props.modelValue.items.filter((_, position) => position !== index))
}

function moveItem(index: number, direction: 'up' | 'down'): void {
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= props.modelValue.items.length) return
  const items = [...props.modelValue.items]
  const [entry] = items.splice(index, 1)
  items.splice(target, 0, entry)
  setItems(items)
}

function addItem(): void {
  setItems([...props.modelValue.items, { id: newDraftId(), label: '', target: null, children: [] }])
}
</script>

<template>
  <div class="navigation-fields">
    <p class="inspector-copy">
      主导航保存在站点配置中，由公共站按顺序渲染。拖拽顺序在这里以「上移 / 下移」按钮提供，保证键盘可操作。
    </p>
    <p v-if="!modelValue.items.length" class="empty-mini">还没有导航项。</p>
    <ul class="navigation-fields__list">
      <NavigationItemEditor
        v-for="(item, index) in modelValue.items"
        :key="item.id"
        :model-value="item"
        :index="index"
        :count="modelValue.items.length"
        @update:model-value="updateItem(index, $event)"
        @remove="removeItem(index)"
        @move="moveItem(index, $event)"
      />
    </ul>
    <button class="button button--quiet" type="button" @click="addItem"><Plus :size="14" />添加顶级导航项</button>
    <p class="navigation-fields__legend">
      <ArrowUp :size="13" /><ArrowDown :size="13" /><Trash2 :size="13" />排序与删除均可通过键盘完成。
    </p>
  </div>
</template>

<style scoped>
@layer components {
.navigation-fields { display: flex; flex-direction: column; gap: 0.6rem; }
.navigation-fields__list { display: flex; flex-direction: column; gap: 0.2rem; margin: 0; padding: 0; }
.navigation-fields__legend { display: flex; align-items: center; gap: 0.25rem; margin: 0; color: var(--text-secondary); font-size: 0.75rem; }
}
</style>
