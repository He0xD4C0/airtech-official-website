<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'

const props = defineProps<{
  open: boolean
  isPlaceholder: boolean
  issues: string[]
  reason: string
  publishing: boolean
}>()

const emit = defineEmits<{
  'update:reason': [value: string]
  confirm: []
  close: []
}>()

const panel = ref<HTMLElement | null>(null)

watch(
  () => props.open,
  async (open) => {
    if (!open) return
    await nextTick()
    panel.value?.focus()
  },
)
</script>

<template>
  <div v-if="open" class="dialog-backdrop" @keydown.esc="emit('close')">
    <div
      ref="panel"
      class="dialog-panel"
      role="dialog"
      aria-modal="true"
      aria-labelledby="publish-dialog-title"
      tabindex="-1"
    >
      <h2 id="publish-dialog-title">发布内容</h2>
      <p v-if="isPlaceholder" class="dialog-note">当前为占位内容：发布后仍保持 noindex，且不会进入 sitemap。</p>
      <ul v-if="issues.length" class="dialog-issues">
        <li v-for="issue in issues" :key="issue">{{ issue }}</li>
      </ul>
      <label class="field"><span>发布原因（10–2000 字符）</span>
        <textarea
          :value="reason"
          rows="2"
          maxlength="2000"
          @input="emit('update:reason', ($event.target as HTMLTextAreaElement).value)"
        />
      </label>
      <div class="dialog-actions">
        <button class="button button--quiet" type="button" @click="emit('close')">取消</button>
        <button
          class="button button--primary"
          type="button"
          :disabled="publishing || reason.trim().length < 10 || issues.length > 0"
          @click="emit('confirm')"
        >{{ publishing ? '正在发布…' : '确认发布' }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dialog-backdrop { position: fixed; inset: 0; display: grid; place-items: center; background: rgba(15, 23, 42, .45); padding: 1rem; z-index: 50; }
.dialog-panel { width: min(560px, 100%); max-height: 85vh; overflow: auto; background: #fff; border-radius: .6rem; padding: 1.1rem; display: flex; flex-direction: column; gap: .75rem; }
.dialog-note { font-size: .85rem; color: #92400e; }
.dialog-issues { margin: 0; padding-left: 1.1rem; font-size: .85rem; color: #b91c1c; }
.dialog-actions { display: flex; justify-content: flex-end; gap: .5rem; }
</style>
