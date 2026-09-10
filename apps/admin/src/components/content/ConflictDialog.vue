<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { CloudDownload, FileJson2, ShieldAlert, TriangleAlert } from 'lucide-vue-next'
import type { ContentDraftV2 } from '@airtek/contracts'
import type { JsonRevisionDifference } from '@/services/jsonRevisionDiff'
import RevisionDiff from './RevisionDiff.vue'

const props = defineProps<{
  open: boolean
  localDraft: ContentDraftV2 | null
  serverDraft: ContentDraftV2 | null
  changes: JsonRevisionDifference[]
}>()

const emit = defineEmits<{
  download: []
  reload: []
  close: []
}>()

const panel = ref<HTMLElement | null>(null)
const closeButton = ref<HTMLButtonElement | null>(null)
const confirmingReload = ref(false)
let previousFocus: HTMLElement | null = null

function focusable(): HTMLElement[] {
  if (!panel.value) return []
  return Array.from(panel.value.querySelectorAll<HTMLElement>(
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  ))
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.stopPropagation()
    emit('close')
    return
  }
  if (event.key !== 'Tab') return
  const items = focusable()
  if (!items.length) return
  const first = items[0]
  const last = items[items.length - 1]
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

function requestReload(): void {
  if (!confirmingReload.value) {
    confirmingReload.value = true
    return
  }
  emit('reload')
}

watch(() => props.open, async (open) => {
  if (open) {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    await nextTick()
    closeButton.value?.focus()
  } else {
    confirmingReload.value = false
    previousFocus?.focus()
    previousFocus = null
  }
})

onBeforeUnmount(() => {
  previousFocus?.focus()
})
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="dialog-backdrop" @keydown="onKeydown">
      <div
        ref="panel"
        class="dialog-panel conflict-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="conflict-title"
        aria-describedby="conflict-summary"
      >
        <header class="conflict-dialog__header">
          <ShieldAlert :size="20" />
          <div>
            <h2 id="conflict-title">保存已停止：检测到并发编辑</h2>
            <p id="conflict-summary">
              服务器草稿在本次编辑期间发生了变化。自动保存已经停止，本地未保存内容仍保留在这个页面里。
            </p>
          </div>
        </header>

        <dl class="conflict-dialog__meta">
          <div>
            <dt>本地草稿</dt>
            <dd>{{ localDraft ? `v${localDraft.draftVersion} · ${localDraft.title || '未命名'}` : '不可用' }}</dd>
          </div>
          <div>
            <dt>服务器草稿</dt>
            <dd>{{ serverDraft ? `v${serverDraft.draftVersion} · ${serverDraft.title || '未命名'}` : '无法读取' }}</dd>
          </div>
        </dl>

        <p v-if="!serverDraft" class="conflict-dialog__warning" role="alert">
          <TriangleAlert :size="15" />无法读取服务器版本，仍可下载本地 JSON 后手动比对。
        </p>

        <RevisionDiff
          :changes="changes"
          empty-message="服务器版本与本地草稿在可比较范围内没有差异。"
        />

        <p v-if="confirmingReload" class="conflict-dialog__warning" role="alert">
          <TriangleAlert :size="15" />重新载入会丢弃本地未保存更改。建议先下载本地 JSON。
        </p>

        <div class="dialog-actions">
          <button ref="closeButton" class="button button--quiet" type="button" @click="emit('close')">
            稍后处理
          </button>
          <button class="button button--secondary" type="button" @click="emit('download')">
            <FileJson2 :size="15" />下载本地 JSON
          </button>
          <button
            class="button"
            :class="confirmingReload ? 'conflict-dialog__danger' : 'button--primary'"
            type="button"
            @click="requestReload"
          >
            <CloudDownload :size="15" />
            {{ confirmingReload ? '确认丢弃并重新载入' : '重新载入服务器版本' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-backdrop { position: fixed; z-index: 70; display: grid; place-items: center; inset: 0; padding: 1.5rem; background: rgba(11, 38, 48, 0.5); }
.dialog-panel { display: flex; flex-direction: column; gap: 0.6rem; width: min(44rem, 94vw); max-height: 90vh; overflow-y: auto; padding: 1.2rem; border-radius: 14px; background: white; box-shadow: 0 28px 70px rgba(11, 38, 48, 0.32); }
.conflict-dialog__header { display: flex; gap: 0.55rem; color: #b42318; }
.conflict-dialog__header h2 { margin: 0 0 0.2rem; font-size: 0.95rem; }
.conflict-dialog__header p { margin: 0; color: var(--admin-text); font-size: 0.65rem; line-height: 1.55; }
.conflict-dialog__meta { display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 0.5rem; margin: 0; }
.conflict-dialog__meta > div { padding: 0.5rem 0.6rem; border: 1px solid var(--admin-line); border-radius: 8px; background: #f8fafa; }
.conflict-dialog__meta dt { color: var(--admin-muted); font-size: 0.54rem; letter-spacing: 0.05em; text-transform: uppercase; }
.conflict-dialog__meta dd { margin: 0.15rem 0 0; font-size: 0.64rem; }
.conflict-dialog__warning { display: flex; align-items: center; gap: 0.4rem; padding: 0.5rem 0.6rem; margin: 0; border: 1px solid #f0dca8; border-radius: 8px; background: var(--admin-soft-amber); color: #8a6100; font-size: 0.62rem; }
.dialog-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 0.4rem; padding-top: 0.6rem; border-top: 1px solid var(--admin-line-soft); }
.button.conflict-dialog__danger { border-color: #d64545; background: #d64545; color: white; }
.button.conflict-dialog__danger:hover:not(:disabled) { border-color: #b33535; background: #b33535; }
</style>
