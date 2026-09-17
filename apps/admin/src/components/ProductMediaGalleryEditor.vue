<script setup lang="ts">
import { ref, watch } from 'vue'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { AssetVersionReference, ProductMediaGalleryItem } from '@airtek/contracts'
import MediaAssetField from '@/components/content/fields/MediaAssetField.vue'

interface GalleryRow {
  key: string
  assetId: string | null
  altText: string
}

const props = defineProps<{ modelValue: ProductMediaGalleryItem[]; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: ProductMediaGalleryItem[]] }>()
const rows = ref<GalleryRow[]>([])

watch(() => props.modelValue, (value) => {
  const external = JSON.stringify(value)
  const current = JSON.stringify(rows.value.flatMap((row) => row.assetId
    ? [{ assetId: row.assetId, altText: row.altText }]
    : []))
  if (external === current) return
  rows.value = value.map((item) => ({ key: crypto.randomUUID(), ...item }))
}, { immediate: true, deep: true })

function sync(): void {
  emit('update:modelValue', rows.value.flatMap((row) => row.assetId
    ? [{ assetId: row.assetId, altText: row.altText }]
    : []))
}

function add(): void {
  if (rows.value.length >= 12) return
  rows.value.push({ key: crypto.randomUUID(), assetId: null, altText: '' })
}

function setAsset(index: number, value: AssetVersionReference | null): void {
  rows.value[index].assetId = value?.assetId ?? null
  sync()
}

function move(index: number, offset: number): void {
  const target = index + offset
  if (target < 0 || target >= rows.value.length) return
  const [row] = rows.value.splice(index, 1)
  rows.value.splice(target, 0, row)
  sync()
}

function remove(index: number): void {
  rows.value.splice(index, 1)
  sync()
}
</script>

<template>
  <div class="product-gallery-editor stack">
    <div class="cluster product-gallery-editor__heading">
      <div><strong>产品图库</strong><small>最多 12 张；第一张是主图。图库仅属于网站展示层。</small></div>
      <button class="button button--secondary" type="button" :disabled="disabled || rows.length >= 12" @click="add">
        <Plus :size="15" />添加图片
      </button>
    </div>
    <p v-if="!rows.length" class="inline-note">尚未配置网站展示图片。</p>
    <article v-for="(row, index) in rows" :key="row.key" class="product-gallery-editor__item">
      <header class="cluster">
        <strong>{{ index === 0 ? '主图' : `图片 ${index + 1}` }}</strong>
        <div class="cluster">
          <button class="icon-button" type="button" :disabled="disabled || index === 0" aria-label="上移图片" @click="move(index, -1)"><ArrowUp :size="15" /></button>
          <button class="icon-button" type="button" :disabled="disabled || index === rows.length - 1" aria-label="下移图片" @click="move(index, 1)"><ArrowDown :size="15" /></button>
          <button class="icon-button" type="button" :disabled="disabled" aria-label="移除图片" @click="remove(index)"><Trash2 :size="15" /></button>
        </div>
      </header>
      <MediaAssetField
        mode="asset"
        label="产品图库图片"
        :model-value="row.assetId ? { assetId: row.assetId } : null"
        :disabled="disabled"
        @update:model-value="setAsset(index, $event as AssetVersionReference | null)"
      />
      <label class="field"><span>替代文本（必填）</span><input v-model="row.altText" maxlength="300" :disabled="disabled" @input="sync" /></label>
    </article>
  </div>
</template>

<style scoped>
@layer components {
  .product-gallery-editor__heading { justify-content: space-between; }
  .product-gallery-editor__heading > div { display: flex; flex-direction: column; }
  .product-gallery-editor__heading small { color: var(--text-secondary); }
  .product-gallery-editor__item { display: grid; gap: var(--space-3); padding: var(--space-4); border: 1px solid var(--border-default); border-radius: 10px; background: var(--surface-subtle); }
  .product-gallery-editor__item > header { justify-content: space-between; }
}
</style>
