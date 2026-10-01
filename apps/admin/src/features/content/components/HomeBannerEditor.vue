<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ContentBlock } from '@airtek/contracts'
import BlockFieldsEditor from '@/features/content/components/blockInspectors/BlockFieldsEditor.vue'
import { defaultBlock } from '@/features/content/services/contentDraftDefaults'

const props = defineProps<{ modelValue: ContentBlock[]; title: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: ContentBlock[]] }>()
const selectedId = ref<string | null>(null)
const banners = computed(() => props.modelValue.filter(block => block.type === 'hero'))
const selected = computed(() => banners.value.find(block => block.id === selectedId.value) ?? banners.value[0])

function update(next: ContentBlock[]): void {
  emit('update:modelValue', [...next, ...props.modelValue.filter(block => block.type !== 'hero')])
}
function add(): void {
  const block = defaultBlock('hero', props.title)
  if (!block) return
  selectedId.value = block.id
  update([...banners.value, block])
}
function move(index: number, offset: number): void {
  const next = [...banners.value]
  const target = index + offset
  if (!next[index] || !next[target]) return
  ;[next[index], next[target]] = [next[target], next[index]]
  update(next)
}
function remove(id: string): void {
  if (banners.value.length > 1) update(banners.value.filter(block => block.id !== id))
}
function edit(block: ContentBlock): void {
  update(banners.value.map(entry => entry.id === block.id ? block : entry))
}
</script>

<template>
  <section class="home-banner-editor" aria-label="首页 Banner 管理">
    <h3>首页 Banner（{{ banners.length }} 页）</h3>
    <p>图片 → 左深右透明渐变 → 白色文字。发布后按此顺序轮播，至少保留一页。</p>
    <ol>
      <li v-for="(banner, index) in banners" :key="banner.id">
        <button type="button" class="button button--quiet" :aria-pressed="selected?.id === banner.id"
          :aria-label="`编辑 Banner ${index + 1}`" @click="selectedId = banner.id">{{ index + 1 }} · {{ banner.heading || '未填写标题' }}</button>
        <div class="home-banner-editor__actions">
          <button class="button button--quiet" type="button" :disabled="index === 0" :aria-label="`上移 Banner ${index + 1}`" @click="move(index, -1)">↑</button>
          <button class="button button--quiet" type="button" :disabled="index === banners.length - 1" :aria-label="`下移 Banner ${index + 1}`" @click="move(index, 1)">↓</button>
          <button class="button button--quiet" type="button" :disabled="banners.length <= 1" :aria-label="`删除 Banner ${index + 1}`" @click="remove(banner.id)">删除</button>
        </div>
      </li>
    </ol>
    <button class="button button--quiet" type="button" @click="add">添加 Banner 页面</button>
    <BlockFieldsEditor v-if="selected" :key="selected.id" :model-value="selected" :relations="[]" home-banner @update:model-value="edit" />
  </section>
</template>

<style scoped>
@layer components {
  .home-banner-editor { display: flex; flex-direction: column; gap: var(--space-3); }
  .home-banner-editor h3, .home-banner-editor p { margin: 0; }
  .home-banner-editor p { color: var(--text-secondary); font-size: .8rem; }
  .home-banner-editor ol { display: flex; flex-direction: column; gap: var(--space-2); padding: 0; list-style: none; }
  .home-banner-editor li, .home-banner-editor__actions { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .home-banner-editor li { justify-content: space-between; padding: var(--space-2); border: 1px solid var(--border-default); border-radius: 8px; }
  .home-banner-editor button[aria-pressed='true'] { color: var(--airtek-blue-dark); outline: 2px solid var(--airtek-blue); }
}
</style>
