<script setup lang="ts">
import { computed, ref } from 'vue'
import { GitCompare, History, RotateCcw, RefreshCcw, Star } from 'lucide-vue-next'
import type { ContentRevisionV2 } from '@airtek/contracts'
import { revisionKindLabels } from './labels'

const props = defineProps<{
  revisions: ContentRevisionV2[]
  currentDraftVersion: number
  publishedRevision: number | null
}>()

const emit = defineEmits<{
  refresh: []
  compare: [baseRevision: number, targetRevision?: number]
  restore: [revision: number, reason: string]
}>()

const RESTORE_REASON_MIN = 10
const RESTORE_REASON_MAX = 2000

const baseRevision = ref<number | null>(null)
const targetRevision = ref<'draft' | number>('draft')
const restoreTarget = ref<ContentRevisionV2 | null>(null)
const restoreReason = ref('')

const sorted = computed(() => [...props.revisions].sort((left, right) => right.revision - left.revision))
const canCompare = computed(() => baseRevision.value !== null)
const canRestore = computed(() => restoreReason.value.trim().length >= RESTORE_REASON_MIN
  && restoreReason.value.trim().length <= RESTORE_REASON_MAX)

const targetOptions = computed(() => sorted.value.filter((revision) => revision.revision !== baseRevision.value))

function formatTimestamp(value: string): string {
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString('zh-CN')
}

function runCompare(): void {
  if (baseRevision.value === null) return
  if (targetRevision.value === 'draft') emit('compare', baseRevision.value)
  else emit('compare', baseRevision.value, targetRevision.value)
}

function openRestore(revision: ContentRevisionV2): void {
  restoreTarget.value = revision
  restoreReason.value = ''
}

function closeRestore(): void {
  restoreTarget.value = null
  restoreReason.value = ''
}

function confirmRestore(): void {
  if (!restoreTarget.value || !canRestore.value) return
  emit('restore', restoreTarget.value.revision, restoreReason.value.trim())
  closeRestore()
}

function selectBase(revision: number, checked: boolean): void {
  baseRevision.value = checked ? revision : null
}
</script>

<template>
  <section class="revision-timeline" aria-labelledby="revision-timeline-title">
    <header class="revision-timeline__header">
      <h2 id="revision-timeline-title"><History :size="15" />修订时间线</h2>
      <button class="button button--quiet" type="button" @click="emit('refresh')">
        <RefreshCcw :size="14" />刷新
      </button>
    </header>

    <p class="revision-timeline__meta">
      当前草稿 v{{ currentDraftVersion }} ·
      <template v-if="publishedRevision !== null">已发布 revision {{ publishedRevision }}</template>
      <template v-else>尚未发布</template>
    </p>

    <fieldset v-if="sorted.length" class="revision-timeline__controls">
      <legend>对比设置</legend>
      <label class="field field--compact">
        <span>基准修订</span>
        <select
          :value="baseRevision ?? ''"
          aria-describedby="revision-compare-hint"
          @change="baseRevision = ($event.target as HTMLSelectElement).value ? Number(($event.target as HTMLSelectElement).value) : null"
        >
          <option value="">未选择</option>
          <option v-for="revision in sorted" :key="revision.revision" :value="revision.revision">
            r{{ revision.revision }} · {{ revisionKindLabels[revision.kind] ?? revision.kind }}
          </option>
        </select>
      </label>
      <label class="field field--compact">
        <span>对比目标</span>
        <select v-model="targetRevision">
          <option value="draft">当前草稿 v{{ currentDraftVersion }}</option>
          <option v-for="revision in targetOptions" :key="revision.revision" :value="revision.revision">
            r{{ revision.revision }} · {{ revisionKindLabels[revision.kind] ?? revision.kind }}
          </option>
        </select>
      </label>
      <button class="button button--secondary" type="button" :disabled="!canCompare" @click="runCompare">
        <GitCompare :size="14" />查看差异
      </button>
      <p id="revision-compare-hint" class="revision-timeline__hint">差异由服务端计算，包含字段级 before / after。</p>
    </fieldset>

    <p v-if="!sorted.length" class="empty-mini">还没有修订。创建手动快照或发布后会出现在这里。</p>

    <ol v-else class="revision-timeline__list">
      <li v-for="revision in sorted" :key="revision.revision" class="revision-timeline__item">
        <div class="revision-timeline__row">
          <label class="revision-timeline__select">
            <input
              type="radio"
              name="revision-base"
              :checked="baseRevision === revision.revision"
              :aria-label="`选择 revision ${revision.revision} 作为对比基准`"
              @change="selectBase(revision.revision, ($event.target as HTMLInputElement).checked)"
            />
          </label>
          <div class="revision-timeline__body">
            <p class="revision-timeline__title">
              <strong>r{{ revision.revision }}</strong>
              <span class="revision-timeline__kind">{{ revisionKindLabels[revision.kind] ?? revision.kind }}</span>
              <span v-if="publishedRevision === revision.revision" class="revision-timeline__published">
                <Star :size="12" />已发布
              </span>
            </p>
            <p class="revision-timeline__details">
              草稿 v{{ revision.sourceDraftVersion }} · {{ revision.createdBy }} · {{ formatTimestamp(revision.createdAt) }}
            </p>
            <p class="revision-timeline__reason">{{ revision.reason }}</p>
          </div>
          <button class="button button--quiet" type="button" @click="openRestore(revision)">
            <RotateCcw :size="14" />恢复
          </button>
        </div>
      </li>
    </ol>

    <div v-if="restoreTarget" class="dialog-backdrop" @keydown.esc="closeRestore">
      <div class="dialog-panel" role="dialog" aria-modal="true" aria-labelledby="restore-title">
        <h3 id="restore-title">恢复 revision {{ restoreTarget.revision }}</h3>
        <p class="dialog-note">
          恢复会基于该修订重建当前草稿；已发布的公开版本不会自动变化，需要再次发布。
        </p>
        <label class="field">
          <span>恢复原因（{{ RESTORE_REASON_MIN }}–{{ RESTORE_REASON_MAX }} 字符）</span>
          <textarea v-model="restoreReason" rows="3" maxlength="2000" />
        </label>
        <div class="dialog-actions">
          <button class="button button--quiet" type="button" @click="closeRestore">取消</button>
          <button class="button button--primary" type="button" :disabled="!canRestore" @click="confirmRestore">
            确认恢复
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.revision-timeline { display: flex; flex-direction: column; gap: 0.6rem; }
.revision-timeline__header { display: flex; align-items: center; justify-content: space-between; }
.revision-timeline__header h2 { display: flex; align-items: center; gap: 0.3rem; margin: 0; font-size: 0.85rem; }
.revision-timeline__meta { margin: 0; color: var(--admin-muted); font-size: 0.6rem; }
.revision-timeline__controls { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 0.4rem; padding: 0.55rem; margin: 0; border: 1px solid var(--admin-line); border-radius: 9px; }
.revision-timeline__controls legend { padding: 0 0.3rem; color: var(--admin-muted); font-size: 0.56rem; }
.revision-timeline__hint { flex-basis: 100%; margin: 0; color: var(--admin-muted); font-size: 0.55rem; }
.revision-timeline__list { display: flex; flex-direction: column; gap: 0.4rem; margin: 0; padding: 0; list-style: none; }
.revision-timeline__item { border: 1px solid var(--admin-line); border-radius: 9px; background: white; }
.revision-timeline__row { display: flex; align-items: flex-start; gap: 0.45rem; padding: 0.5rem; }
.revision-timeline__select { display: flex; align-items: center; padding-top: 0.15rem; }
.revision-timeline__body { flex: 1; min-width: 0; }
.revision-timeline__title { display: flex; align-items: center; gap: 0.35rem; margin: 0; font-size: 0.66rem; }
.revision-timeline__kind { padding: 0.08rem 0.3rem; border-radius: 5px; background: var(--admin-soft-blue); color: var(--airtek-blue-dark); font-size: 0.55rem; }
.revision-timeline__published { display: inline-flex; align-items: center; gap: 0.2rem; color: #2f7d32; font-size: 0.55rem; }
.revision-timeline__details { margin: 0.15rem 0 0; color: var(--admin-muted); font-size: 0.56rem; }
.revision-timeline__reason { margin: 0.2rem 0 0; font-size: 0.6rem; overflow-wrap: anywhere; }
.dialog-backdrop { position: fixed; z-index: 60; display: grid; place-items: center; inset: 0; padding: 1.5rem; background: rgba(11, 38, 48, 0.45); }
.dialog-panel { display: flex; flex-direction: column; gap: 0.5rem; width: min(34rem, 92vw); padding: 1.1rem; border-radius: 14px; background: white; }
.dialog-panel h3 { margin: 0; font-size: 0.9rem; }
.dialog-note { margin: 0; color: var(--admin-muted); font-size: 0.62rem; line-height: 1.5; }
.dialog-actions { display: flex; justify-content: flex-end; gap: 0.4rem; padding-top: 0.5rem; border-top: 1px solid var(--admin-line-soft); }
</style>
