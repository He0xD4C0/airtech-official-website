<script setup lang="ts">
import { computed, ref } from 'vue'
import { LoaderCircle, Plus, Search, Trash2 } from 'lucide-vue-next'
import type { CollectionPresentation, CmsPublishedContent, ContentRelationReference, Product, RelationCollectionBlock, RelationTargetReference } from '@airtek/contracts'
import { contentApi } from '@/features/content/services/contentApi'
import { apiErrorMessage } from '@/shared/services/cursorPagination'

const props = defineProps<{
  modelValue: RelationCollectionBlock
  relations: ContentRelationReference[]
}>()

const emit = defineEmits<{
  'update:modelValue': [value: RelationCollectionBlock]
  'add-relation': [target: RelationTargetReference, slot: string]
}>()

const PRESENTATIONS: CollectionPresentation[] = ['cards', 'list', 'compact']
const pickerOpen = ref(false)
const query = ref('')
const contentResults = ref<CmsPublishedContent[]>([])
const productResults = ref<Product[]>([])
const searchState = ref<'idle' | 'loading' | 'ready' | 'empty' | 'error'>('idle')
const searchError = ref('')
let debounce: ReturnType<typeof setTimeout> | undefined

/** Relations owned by this block: referenced by id, or attached to this block as their slot. */
const blockRelations = computed(() => props.relations.filter((relation) => (
  props.modelValue.relationIds.includes(relation.id) || relation.slot === props.modelValue.id
)))

function relationLabel(relation: ContentRelationReference): string {
  const record = contentResults.value.find((entry) => entry.contentId === (
    relation.target.targetType === 'content' ? relation.target.contentId : ''
  ))
  if (record) return record.document.title
  const product = productResults.value.find((entry) => entry.id === (
    relation.target.targetType === 'product' ? relation.target.productId : ''
  ))
  if (product) return product.seo?.title || product.model || `产品 ${product.id.slice(0, 8)}…`
  return relation.target.targetType === 'content'
    ? `内容 ${relation.target.contentId.slice(0, 8)}…`
    : `产品 ${relation.target.productId.slice(0, 8)}…`
}

function patch(values: Partial<RelationCollectionBlock>): void {
  emit('update:modelValue', { ...props.modelValue, ...values })
}

function removeRelation(relation: ContentRelationReference): void {
  patch({ relationIds: props.modelValue.relationIds.filter((id) => id !== relation.id) })
}

function canRemove(relation: ContentRelationReference): boolean {
  return props.modelValue.relationIds.includes(relation.id)
}

async function search(): Promise<void> {
  searchState.value = 'loading'
  searchError.value = ''
  try {
    const [contentPage, productPage] = await Promise.all([
      contentApi.listPublished({ q: query.value.trim() || undefined, limit: 6 }).catch(() => null),
      contentApi.searchProducts({ q: query.value.trim() || undefined, limit: 6 }).catch(() => null),
    ])
    contentResults.value = contentPage?.items ?? []
    productResults.value = productPage?.items ?? []
    if (!contentPage && !productPage) {
      searchState.value = 'error'
      searchError.value = '内容与产品搜索都不可用，请稍后重试。'
      return
    }
    searchState.value = contentResults.value.length || productResults.value.length ? 'ready' : 'empty'
  } catch (error) {
    searchState.value = 'error'
    searchError.value = apiErrorMessage(error, '无法搜索可关联的实体。')
  }
}

function scheduleSearch(): void {
  if (debounce !== undefined) clearTimeout(debounce)
  debounce = setTimeout(() => {
    debounce = undefined
    void search()
  }, 320)
}

function openPicker(): void {
  pickerOpen.value = true
  void search()
}

function closePicker(): void {
  pickerOpen.value = false
  query.value = ''
  contentResults.value = []
  productResults.value = []
  searchState.value = 'idle'
}

function chooseContent(record: CmsPublishedContent): void {
  emit('add-relation', { targetType: 'content', contentId: record.contentId }, props.modelValue.id)
  closePicker()
}

function chooseProduct(product: Product): void {
  emit('add-relation', { targetType: 'product', productId: product.id }, props.modelValue.id)
  closePicker()
}

function onQueryInput(value: string): void {
  query.value = value
  scheduleSearch()
}

defineExpose({ blockRelations })
</script>

<template>
  <div class="relation-collection">
    <label class="field">
      <span>区块标题</span>
      <input
        :value="modelValue.heading ?? ''"
        maxlength="200"
        @input="patch({ heading: ($event.target as HTMLInputElement).value || null })"
      />
    </label>
    <label class="field">
      <span>呈现方式</span>
      <select :value="modelValue.presentation" @change="patch({ presentation: ($event.target as HTMLSelectElement).value as CollectionPresentation })">
        <option v-for="option in PRESENTATIONS" :key="option" :value="option">{{ option }}</option>
      </select>
    </label>

    <div class="relation-collection__header">
      <strong>关联实体（{{ blockRelations.length }}）</strong>
      <button class="button button--quiet" type="button" @click="pickerOpen ? closePicker() : openPicker()">
        <Plus :size="14" />{{ pickerOpen ? '收起' : '添加关联' }}
      </button>
    </div>
    <p v-if="!blockRelations.length" class="empty-mini">还没有关联实体。</p>
    <ul v-else class="relation-collection__list">
      <li v-for="relation in blockRelations" :key="relation.id">
        <span class="relation-collection__badge">{{ relation.target.targetType === 'content' ? '内容' : '产品' }}</span>
        <span class="relation-collection__label">{{ relationLabel(relation) }}</span>
        <button
          v-if="canRemove(relation)"
          class="icon-button"
          type="button"
          :aria-label="`移除关联 ${relationLabel(relation)}`"
          @click="removeRelation(relation)"
        >
          <Trash2 :size="14" />
        </button>
        <span
          v-else
          class="relation-collection__managed"
          :title="'该关联由内容关联记录（draft.relations）管理，需在内容记录中移除'"
        >受内容记录管理</span>
      </li>
    </ul>

    <div v-if="pickerOpen" class="relation-collection__picker">
      <label class="search-field">
        <Search :size="15" />
        <span class="sr-only">搜索内容或产品</span>
        <input
          type="search"
          :value="query"
          placeholder="搜索标题或产品型号"
          @input="onQueryInput(($event.target as HTMLInputElement).value)"
        />
      </label>
      <p v-if="searchState === 'loading'" class="relation-collection__state"><LoaderCircle class="data-state__spin" :size="14" />正在搜索…</p>
      <p v-else-if="searchState === 'error'" class="relation-collection__state relation-collection__state--error">{{ searchError }}</p>
      <p v-else-if="searchState === 'empty'" class="relation-collection__state">没有匹配的内容或产品。</p>
      <template v-else-if="searchState === 'ready'">
        <p v-if="contentResults.length" class="relation-collection__group">内容</p>
        <ul class="relation-collection__results">
          <li v-for="record in contentResults" :key="record.contentId">
            <button type="button" @click="chooseContent(record)">
              <strong>{{ record.document.title }}</strong>
              <small>{{ record.document.kind }}</small>
            </button>
          </li>
        </ul>
        <p v-if="productResults.length" class="relation-collection__group">产品</p>
        <ul class="relation-collection__results">
          <li v-for="product in productResults" :key="product.id">
            <button type="button" @click="chooseProduct(product)">
              <strong>{{ product.model ?? product.family }}</strong>
              <small>{{ product.family }}</small>
            </button>
          </li>
        </ul>
      </template>
    </div>
  </div>
</template>

<style scoped>
@layer components {
.relation-collection { display: flex; flex-direction: column; gap: 0.55rem; }
.relation-collection__header { display: flex; align-items: center; justify-content: space-between; }
.relation-collection__header strong { font-size: 0.75rem; }
.relation-collection__list { display: flex; flex-direction: column; gap: 0.3rem; margin: 0; padding: 0; list-style: none; }
.relation-collection__list > li { display: flex; align-items: center; gap: 0.4rem; padding: 0.4rem 0.5rem; border: 1px solid var(--border-default); border-radius: 7px; background: white; }
.relation-collection__badge { padding: 0.1rem 0.35rem; border-radius: 5px; background: var(--surface-info); color: var(--airtek-blue-dark); font-size: 0.75rem; }
.relation-collection__label { flex: 1; overflow: hidden; font-size: 0.75rem; text-overflow: ellipsis; white-space: nowrap; }
.relation-collection__managed { color: var(--text-secondary); font-size: 0.75rem; }
.relation-collection__picker { display: flex; flex-direction: column; gap: 0.4rem; padding: 0.55rem; border: 1px solid var(--border-default); border-radius: 9px; background: #f8fafa; }
.relation-collection__state { display: flex; align-items: center; gap: 0.35rem; margin: 0; color: var(--text-secondary); font-size: 0.75rem; }
.relation-collection__state--error { color: #b42318; }
.relation-collection__group { margin: 0.2rem 0 0; color: var(--text-secondary); font-size: 0.75rem; letter-spacing: 0.06em; text-transform: uppercase; }
.relation-collection__results { display: flex; flex-direction: column; gap: 0.25rem; margin: 0; padding: 0; list-style: none; }
.relation-collection__results button { display: flex; align-items: baseline; justify-content: space-between; gap: 0.4rem; width: 100%; padding: 0.4rem 0.5rem; border: 1px solid var(--border-default); border-radius: 7px; background: white; text-align: left; }
.relation-collection__results button:hover { border-color: var(--airtek-blue); background: var(--surface-info); }
.relation-collection__results strong { font-size: 0.75rem; }
.relation-collection__results small { color: var(--text-secondary); font-size: 0.75rem; }
}
</style>
