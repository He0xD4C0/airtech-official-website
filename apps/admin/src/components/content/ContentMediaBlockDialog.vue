<script setup lang="ts">
import { ref } from 'vue'
import type { AssetVersionReference, ContentBlockKind, MediaUseReference } from '@airtek/contracts'
import MediaAssetField from '@/components/content/fields/MediaAssetField.vue'

defineProps<{ kind: ContentBlockKind | null }>()
const emit = defineEmits<{
  confirm: [asset: AssetVersionReference]
  close: []
}>()

const pending = ref<MediaUseReference | AssetVersionReference | null>(null)

function confirm(): void {
  const value = pending.value
  if (!value) return
  emit('confirm', 'asset' in value ? value.asset : value)
}
</script>

<template>
  <div v-if="kind" class="dialog-backdrop" @keydown.esc="emit('close')">
    <div class="dialog-panel" role="dialog" aria-modal="true" aria-labelledby="media-dialog-title">
      <h2 id="media-dialog-title">选择{{ kind === 'media' ? '媒体' : '下载文件' }}资产</h2>
      <MediaAssetField
        :model-value="pending"
        :mode="kind === 'media' ? 'media' : 'asset'"
        :label="kind === 'media' ? '媒体资产' : '下载文件资产'"
        @update:model-value="pending = $event"
      />
      <div class="dialog-actions">
        <button class="button button--quiet" type="button" @click="emit('close')">取消</button>
        <button class="button button--primary" type="button" :disabled="!pending" @click="confirm">加入区块</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; inset: 0; display: grid; place-items: center; background: rgba(15, 23, 42, .45); padding: 1rem; z-index: 50; }
.dialog-panel { width: min(560px, 100%); max-height: 85vh; overflow: auto; background: #fff; border-radius: .6rem; padding: 1.1rem; display: flex; flex-direction: column; gap: .75rem; }
.dialog-actions { display: flex; justify-content: flex-end; gap: .5rem; }
</style>
