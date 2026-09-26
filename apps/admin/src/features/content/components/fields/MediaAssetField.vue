<script setup lang="ts">
import { computed, getCurrentInstance, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { ImageIcon, LoaderCircle, RefreshCcw, Search, Trash2, Upload } from 'lucide-vue-next'
import type { AssetVersionReference, MediaAsset, MediaUseReference } from '@airtek/contracts'
import { contentApi } from '@/features/content/services/contentApi'
import { apiErrorMessage } from '@/shared/services/cursorPagination'
import { getMediaAsset } from '@/features/media'
import { readImageMetadata, siteIconAssetIssue, siteIconMetadataIssue } from '@/features/content/services/siteIconAsset'
import { PENDING_MEDIA_PREFIX, useDeferredMediaUploads } from '@/features/content/stores/deferredMediaUploads'

type MediaFieldValue = MediaUseReference | AssetVersionReference | null

const props = withDefaults(defineProps<{
  modelValue: MediaFieldValue
  mode?: 'media' | 'asset'
  label?: string
  disabled?: boolean
  siteIcon?: boolean
}>(), {
  mode: 'media',
  label: '媒体资产',
  disabled: false,
  siteIcon: false,
})

const emit = defineEmits<{ 'update:modelValue': [value: MediaFieldValue] }>()
// The field also renders in read-only SSR previews and component audits where
// no application Pinia exists. Deferred uploads are an editor-only capability.
const activePinia = getCurrentInstance()?.appContext.config.globalProperties.$pinia
const deferredMedia = activePinia ? useDeferredMediaUploads(activePinia) : null

const dialogOpen = ref(false)
const query = ref('')
const options = ref<MediaAsset[]>([])
const listState = ref<'idle' | 'loading' | 'ready' | 'empty' | 'error'>('idle')
const listError = ref('')
const searchInput = ref<HTMLInputElement | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)
const selectedAsset = ref<MediaAsset | null>(null)
const constraintError = ref('')
let previousFocus: HTMLElement | null = null
let debounce: ReturnType<typeof setTimeout> | undefined

const selected = computed(() => {
  const value = props.modelValue
  if (!value) return null
  const asset = 'asset' in value ? value.asset : value
  return {
    assetId: asset.assetId,
    altText: 'altText' in value ? value.altText ?? '' : '',
    decorative: 'decorative' in value ? value.decorative : false,
  }
})

const selectionLabel = computed(() => {
  if (!selected.value) return '未选择资产'
  const pending = deferredMedia?.entries[selected.value.assetId]
  if (pending) return `${pending.file.name}（待保存上传）`
  const name = selectedAsset.value?.originalName
    ?? options.value.find((entry) => entry.id === selected.value?.assetId)?.originalName
  return name ?? `资产 ${selected.value.assetId.slice(0, 8)}`
})
const previewSrc = computed(() => selected.value
  ? deferredMedia?.objectUrls[selected.value.assetId]
    ?? selectedAsset.value?.previewUrl
    ?? selectedAsset.value?.publicUrl
  : undefined)

function optionIssue(option: MediaAsset): string | null {
  return props.siteIcon ? siteIconAssetIssue(option) : null
}

function mediaValue(assetId: string): MediaUseReference {
  return {
    asset: { assetId },
    altText: selected.value?.altText ?? null,
    decorative: selected.value?.decorative ?? false,
  }
}

function selectAsset(option: MediaAsset): void {
  const issue = optionIssue(option)
  if (issue) {
    constraintError.value = issue
    return
  }
  discardSelectedPending()
  selectedAsset.value = option
  constraintError.value = ''
  emit('update:modelValue', props.mode === 'asset'
    ? { assetId: option.id }
    : mediaValue(option.id))
  closeDialog()
}

function updateAltText(value: string): void {
  if (!selected.value || props.mode === 'asset') return
  emit('update:modelValue', {
    asset: { assetId: selected.value.assetId },
    altText: value || null,
    decorative: selected.value.decorative,
  })
}

function updateDecorative(value: boolean): void {
  if (!selected.value || props.mode === 'asset') return
  emit('update:modelValue', {
    asset: { assetId: selected.value.assetId },
    altText: value ? null : selected.value.altText || null,
    decorative: value,
  })
}

function clearSelection(): void {
  discardSelectedPending()
  selectedAsset.value = null
  constraintError.value = ''
  emit('update:modelValue', null)
}

function discardSelectedPending(): void {
  const id = selected.value?.assetId
  if (id?.startsWith(PENDING_MEDIA_PREFIX)) deferredMedia?.discard(id)
}

function chooseLocalFile(): void {
  if (!props.disabled) fileInput.value?.click()
}

async function selectLocalFile(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file || !deferredMedia) return
  if (props.siteIcon) {
    try {
      const issue = siteIconMetadataIssue(await readImageMetadata(file))
      if (issue) {
        constraintError.value = issue
        return
      }
    } catch (error) {
      constraintError.value = error instanceof Error ? error.message : '无法验证站点图标。'
      return
    }
  }
  discardSelectedPending()
  selectedAsset.value = null
  constraintError.value = ''
  const assetId = deferredMedia.register(file)
  emit('update:modelValue', props.mode === 'asset'
    ? { assetId }
    : mediaValue(assetId))
}

async function loadOptions(): Promise<void> {
  listState.value = 'loading'
  listError.value = ''
  try {
    const page = await contentApi.listMediaAssets({
      q: query.value.trim() || undefined,
      limit: 50,
    })
    options.value = page.items
    listState.value = options.value.length ? 'ready' : 'empty'
  } catch (error) {
    options.value = []
    listState.value = 'error'
    listError.value = apiErrorMessage(error, '无法读取媒体资产库。')
  }
}

function scheduleSearch(): void {
  if (debounce !== undefined) clearTimeout(debounce)
  debounce = setTimeout(() => {
    debounce = undefined
    void loadOptions()
  }, 300)
}

function openDialog(): void {
  if (props.disabled) return
  previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  dialogOpen.value = true
  void nextTick(() => searchInput.value?.focus())
  void loadOptions()
}

function closeDialog(): void {
  dialogOpen.value = false
  if (debounce !== undefined) {
    clearTimeout(debounce)
    debounce = undefined
  }
  previousFocus?.focus()
  previousFocus = null
}

function onDialogKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.stopPropagation()
    closeDialog()
  }
}

watch(query, scheduleSearch)
watch(() => selected.value?.assetId, async (assetId) => {
  selectedAsset.value = null
  constraintError.value = ''
  if (!assetId || assetId.startsWith(PENDING_MEDIA_PREFIX)) return
  try {
    const asset = await getMediaAsset(assetId)
    if (selected.value?.assetId !== assetId) return
    selectedAsset.value = asset
    constraintError.value = optionIssue(asset) ?? ''
  } catch (error) {
    if (selected.value?.assetId !== assetId) return
    constraintError.value = apiErrorMessage(error, '无法验证已选择的媒体资产。')
  }
}, { immediate: true })
onBeforeUnmount(() => {
  if (debounce !== undefined) clearTimeout(debounce)
})

defineExpose({ loadOptions })
</script>

<template>
  <div class="media-field">
    <div class="media-field__row">
      <span class="media-field__icon" aria-hidden="true"><ImageIcon :size="15" /></span>
      <img v-if="previewSrc" class="media-field__preview" :src="previewSrc" alt="" />
      <div class="media-field__summary">
        <strong>{{ selectionLabel }}</strong>
        <small v-if="selected">ID {{ selected.assetId }}</small>
        <small v-else>媒体只能从资产库选择，不接受手填 UUID 或 URL。</small>
      </div>
      <div class="media-field__actions">
        <input
          ref="fileInput"
          class="sr-only"
          type="file"
          accept="image/png,image/jpeg,image/webp"
          :aria-label="`选择${label}本地图片`"
          :disabled="disabled"
          @change="selectLocalFile"
        />
        <button class="button button--quiet" type="button" :disabled="disabled" @click="chooseLocalFile">
          <Upload :size="15" />本地图片
        </button>
        <button class="button button--quiet" type="button" :disabled="disabled" @click="openDialog">
          {{ selected ? '更换' : '选择' }}
        </button>
        <button
          v-if="selected"
          class="icon-button"
          type="button"
          :disabled="disabled"
          :aria-label="`移除${label}`"
          @click="clearSelection"
        >
          <Trash2 :size="15" />
        </button>
      </div>
    </div>
    <p v-if="constraintError" class="media-field__error" role="alert">{{ constraintError }}</p>

    <label v-if="selected && mode === 'media'" class="field media-field__alt">
      <span>替代文本<small v-if="selected.decorative">装饰性媒体已禁用替代文本</small></span>
      <input
        :value="selected.altText"
        :disabled="disabled || selected.decorative"
        maxlength="300"
        :placeholder="selected.decorative ? '装饰性媒体不需要替代文本' : '描述图片传达的信息'"
        @input="updateAltText(($event.target as HTMLInputElement).value)"
      />
    </label>
    <label v-if="selected && mode === 'media'" class="toggle-row">
      <span><strong>装饰性媒体</strong><small>纯装饰内容对辅助技术隐藏，并强制清空替代文本</small></span>
      <input
        type="checkbox"
        :checked="selected.decorative"
        :disabled="disabled"
        @change="updateDecorative(($event.target as HTMLInputElement).checked)"
      />
    </label>

    <Teleport to="body">
      <div v-if="dialogOpen" class="dialog-backdrop" @keydown="onDialogKeydown">
        <div
          class="dialog-panel media-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="media-asset-title"
          aria-describedby="media-asset-hint"
        >
          <h2 id="media-asset-title">选择{{ label }}</h2>
          <p id="media-asset-hint" class="dialog-note">所有上传成功的图片都可立即选择；内容发布状态只控制网站是否展示。</p>
          <label class="search-field media-dialog__search">
            <Search :size="16" />
            <span class="sr-only">搜索媒体资产</span>
            <input ref="searchInput" v-model="query" type="search" placeholder="按文件名搜索" />
          </label>

          <div class="media-dialog__body" aria-live="polite">
            <p v-if="listState === 'loading'" class="media-dialog__state">
              <LoaderCircle class="data-state__spin" :size="16" />正在读取媒体资产…
            </p>
            <p v-else-if="listState === 'error'" class="media-dialog__state media-dialog__state--error">
              {{ listError }}
            </p>
            <p v-else-if="listState === 'empty'" class="media-dialog__state">没有匹配的媒体资产。</p>
            <ul v-else-if="listState === 'ready'" class="media-dialog__list">
              <li v-for="option in options" :key="option.id">
                <button
                  class="media-dialog__option"
                  type="button"
                  :disabled="Boolean(optionIssue(option))"
                  :title="optionIssue(option) ?? undefined"
                  @click="selectAsset(option)"
                >
                  <span>
                    <strong>{{ option.originalName }}</strong>
                    <small>{{ option.mediaType }} · {{ Math.max(1, Math.round(option.byteSize / 1024)) }} KB</small>
                  </span>
                  <em>{{ optionIssue(option) ?? '可公开使用' }}</em>
                </button>
              </li>
            </ul>
          </div>

          <div class="dialog-actions">
            <button
              class="button button--quiet"
              type="button"
              :disabled="listState === 'loading'"
              @click="loadOptions"
            >
              <RefreshCcw :size="15" />重新加载
            </button>
            <button class="button button--secondary" type="button" @click="closeDialog">关闭</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
@layer components {
.dialog-backdrop { position: fixed; z-index: 60; display: grid; place-items: center; inset: 0; padding: 1.5rem; background: rgba(11, 38, 48, 0.45); }
.dialog-panel { display: flex; flex-direction: column; gap: 0.5rem; max-height: 88vh; padding: 1.1rem; border-radius: 14px; background: white; box-shadow: 0 24px 60px rgba(11, 38, 48, 0.28); }
.dialog-panel h2 { margin: 0; font-size: 0.95rem; }
.dialog-note { margin: 0; color: var(--text-secondary); font-size: 0.75rem; line-height: 1.5; }
.dialog-actions { display: flex; justify-content: flex-end; gap: 0.4rem; padding-top: 0.5rem; border-top: 1px solid var(--border-subtle); }
.media-field { display: flex; flex-direction: column; gap: 0.55rem; }
.media-field__row { display: flex; align-items: center; gap: 0.55rem; padding: 0.6rem; border: 1px solid var(--border-default); border-radius: 8px; background: white; }
.media-field__icon { display: grid; place-items: center; width: 1.9rem; height: 1.9rem; border-radius: 7px; background: var(--surface-info); color: var(--airtek-blue); }
.media-field__preview { width: 2.75rem; height: 2.75rem; border-radius: 7px; object-fit: cover; }
.media-field__summary { display: flex; flex: 1; min-width: 0; flex-direction: column; }
.media-field__summary strong { overflow: hidden; font-size: 0.75rem; text-overflow: ellipsis; white-space: nowrap; }
.media-field__summary small { color: var(--text-secondary); font-size: 0.75rem; }
.media-field__actions { display: flex; align-items: center; gap: 0.25rem; }
.media-field__alt { margin-top: 0; }
.media-field__error { margin: 0; color: #b42318; font-size: 0.75rem; font-weight: 700; }
.media-dialog { width: min(38rem, 92vw); }
.media-dialog__search { margin: 0.6rem 0; }
.media-dialog__body { max-height: 18rem; overflow-y: auto; }
.media-dialog__state { display: flex; align-items: center; gap: 0.4rem; padding: 1.1rem 0; color: var(--text-secondary); font-size: 0.75rem; text-align: center; }
.media-dialog__state--error { color: #b42318; }
.media-dialog__list { display: flex; flex-direction: column; gap: 0.35rem; margin: 0; padding: 0; list-style: none; }
.media-dialog__option { display: flex; align-items: center; justify-content: space-between; gap: 0.6rem; width: 100%; padding: 0.55rem 0.65rem; border: 1px solid var(--border-default); border-radius: 8px; background: white; text-align: left; }
.media-dialog__option:hover:not(:disabled) { border-color: var(--airtek-blue); background: var(--surface-info); }
.media-dialog__option:disabled { cursor: not-allowed; opacity: 0.6; }
.media-dialog__option strong { display: block; font-size: 0.75rem; }
.media-dialog__option small { color: var(--text-secondary); font-size: 0.75rem; }
.media-dialog__option em { color: var(--text-secondary); font-size: 0.75rem; font-style: normal; text-transform: uppercase; }
}
</style>
