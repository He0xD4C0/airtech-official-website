<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Link2, LoaderCircle, Search } from 'lucide-vue-next'
import type { ContentRecordV2, LinkTargetReference } from '@airtek/contracts'
import { contentApi } from '@/services/contentApi'
import { apiErrorMessage } from '@/services/cursorPagination'

const props = defineProps<{
  modelValue: LinkTargetReference
  label?: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: LinkTargetReference]
}>()

const query = ref('')
const results = ref<ContentRecordV2[]>([])
const searchState = ref<'idle' | 'loading' | 'ready' | 'empty' | 'error'>('idle')
const searchError = ref('')
const resolvedLabel = ref('')
let debounce: ReturnType<typeof setTimeout> | undefined

const targetType = computed(() => props.modelValue.targetType)
const contentId = computed(() => (
  props.modelValue.targetType === 'content' ? props.modelValue.contentId : ''
))

const selectionLabel = computed(() => {
  if (props.modelValue.targetType !== 'content') return ''
  return resolvedLabel.value || `内容 ${props.modelValue.contentId.slice(0, 8)}…`
})

function switchType(next: LinkTargetReference['targetType']): void {
  if (next === props.modelValue.targetType) return
  if (next === 'route') emit('update:modelValue', { targetType: 'route', path: '' })
  else if (next === 'external') emit('update:modelValue', { targetType: 'external', url: '' })
  else emit('update:modelValue', { targetType: 'content', contentId: '' })
}

function updatePath(value: string): void {
  if (props.modelValue.targetType !== 'route') return
  emit('update:modelValue', { targetType: 'route', path: value })
}

function updateUrl(value: string): void {
  if (props.modelValue.targetType !== 'external') return
  emit('update:modelValue', { targetType: 'external', url: value })
}

function chooseContent(record: ContentRecordV2): void {
  resolvedLabel.value = record.draft.title
  emit('update:modelValue', { targetType: 'content', contentId: record.id })
  results.value = []
  searchState.value = 'idle'
  query.value = ''
}

async function search(queryText: string): Promise<void> {
  searchState.value = 'loading'
  searchError.value = ''
  try {
    const page = await contentApi.listContent({ q: queryText.trim() || undefined, limit: 8 })
    results.value = page.items
    searchState.value = results.value.length ? 'ready' : 'empty'
  } catch (error) {
    results.value = []
    searchState.value = 'error'
    searchError.value = apiErrorMessage(error, '无法搜索内容。')
  }
}

function scheduleSearch(value: string): void {
  if (debounce !== undefined) clearTimeout(debounce)
  debounce = setTimeout(() => {
    debounce = undefined
    void search(value)
  }, 320)
}

watch(query, (value) => scheduleSearch(value))

watch(contentId, async (id) => {
  resolvedLabel.value = ''
  if (!id) return
  try {
    const { record } = await contentApi.getContentRecord(id)
    resolvedLabel.value = record.draft.title
  } catch {
    resolvedLabel.value = ''
  }
}, { immediate: true })
</script>

<template>
  <div class="link-target">
    <label class="field">
      <span>链接类型</span>
      <select :value="targetType" @change="switchType(($event.target as HTMLSelectElement).value as LinkTargetReference['targetType'])">
        <option value="route">站内路由</option>
        <option value="content">站内内容（受管实体）</option>
        <option value="external">外部链接</option>
      </select>
    </label>

    <label v-if="modelValue.targetType === 'route'" class="field">
      <span>路由路径<small>必须以 / 开头，例如 /en/products</small></span>
      <input
        :value="modelValue.path"
        placeholder="/en/products"
        @input="updatePath(($event.target as HTMLInputElement).value)"
      />
    </label>

    <label v-else-if="modelValue.targetType === 'external'" class="field">
      <span>外部 URL<small>仅 http/https</small></span>
      <input
        :value="modelValue.url"
        type="url"
        placeholder="https://example.com"
        @input="updateUrl(($event.target as HTMLInputElement).value)"
      />
    </label>

    <div v-else class="link-target__content">
      <p class="link-target__current">
        <Link2 :size="14" />
        <span>{{ selectionLabel || '尚未选择内容实体' }}</span>
      </p>
      <p class="link-target__hint">内容实体只能通过搜索选择，不接受手填 UUID。</p>
      <label class="search-field">
        <Search :size="16" />
        <span class="sr-only">搜索内容实体</span>
        <input v-model="query" type="search" placeholder="搜索标题" />
      </label>
      <p v-if="searchState === 'loading'" class="link-target__state">
        <LoaderCircle class="data-state__spin" :size="14" />正在搜索…
      </p>
      <p v-else-if="searchState === 'error'" class="link-target__state link-target__state--error">{{ searchError }}</p>
      <p v-else-if="searchState === 'empty'" class="link-target__state">没有匹配的内容。</p>
      <ul v-else-if="searchState === 'ready'" class="link-target__results">
        <li v-for="record in results" :key="record.id">
          <button type="button" @click="chooseContent(record)">
            <strong>{{ record.draft.title }}</strong>
            <small>{{ record.draft.kind }} · v{{ record.draft.draftVersion }}</small>
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.link-target { display: flex; flex-direction: column; gap: 0.5rem; }
.link-target__content { display: flex; flex-direction: column; gap: 0.45rem; }
.link-target__current { display: flex; align-items: center; gap: 0.35rem; margin: 0; color: var(--airtek-blue-dark); font-size: 0.64rem; }
.link-target__hint { margin: 0; color: var(--admin-muted); font-size: 0.58rem; }
.link-target__state { display: flex; align-items: center; gap: 0.35rem; margin: 0; color: var(--admin-muted); font-size: 0.62rem; }
.link-target__state--error { color: #b42318; }
.link-target__results { display: flex; flex-direction: column; gap: 0.3rem; max-height: 12rem; overflow-y: auto; margin: 0; padding: 0; list-style: none; }
.link-target__results button { display: flex; flex-direction: column; width: 100%; padding: 0.45rem 0.55rem; border: 1px solid var(--admin-line); border-radius: 7px; background: white; text-align: left; }
.link-target__results button:hover { border-color: var(--airtek-blue); background: var(--admin-soft-blue); }
.link-target__results strong { font-size: 0.64rem; }
.link-target__results small { color: var(--admin-muted); font-size: 0.55rem; }
</style>
