<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { getPublishedNews } from '@/shared/lib/api'
import { publishedNewsCard } from '@/features/content/lib/newsCard'
import type { CardEntry, PublicPageModel } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
const newsPageSize = 48
const category = ref('all')
const pageEntries = ref<CardEntry[]>([...(props.page.entries ?? [])])
const pageNextCursor = ref<string | null>(props.page.newsNextCursor ?? null)
const cursorHistory = ref<Array<string | null>>([null])
const pageIndex = ref(0)
const loading = ref(false)
const pageError = ref('')
let requestGeneration = 0

const categories = computed(() => [...new Set(pageEntries.value.flatMap((entry) => entry.category ? [entry.category] : []))].sort())
const entries = computed(() => pageEntries.value.filter((entry) => category.value === 'all' || entry.category === category.value))

watch(
  [() => props.page.entries, () => props.page.newsNextCursor],
  () => {
    requestGeneration += 1
    pageEntries.value = [...(props.page.entries ?? [])]
    pageNextCursor.value = props.page.newsNextCursor ?? null
    cursorHistory.value = [null]
    pageIndex.value = 0
    category.value = 'all'
    loading.value = false
    pageError.value = ''
  },
)

async function loadPage(cursor: string | null): Promise<boolean> {
  const generation = ++requestGeneration
  loading.value = true
  pageError.value = ''
  try {
    const page = await getPublishedNews({ locale: 'en', limit: newsPageSize, ...(cursor ? { cursor } : {}) })
    if (generation !== requestGeneration) return false
    pageEntries.value = page.items.flatMap((entry) => {
      const card = publishedNewsCard(entry)
      return card ? [card] : []
    })
    pageNextCursor.value = page.nextCursor
    category.value = 'all'
    return true
  } catch {
    if (generation === requestGeneration) {
      pageError.value = t('content.newsError')
    }
    return false
  } finally {
    if (generation === requestGeneration) loading.value = false
  }
}

async function nextPage(): Promise<void> {
  const cursor = pageNextCursor.value
  if (!cursor || loading.value) return
  if (await loadPage(cursor)) {
    cursorHistory.value = [...cursorHistory.value.slice(0, pageIndex.value + 1), cursor]
    pageIndex.value += 1
  }
}

async function previousPage(): Promise<void> {
  if (pageIndex.value === 0 || loading.value) return
  const targetIndex = pageIndex.value - 1
  if (await loadPage(cursorHistory.value[targetIndex] ?? null)) pageIndex.value = targetIndex
}

function visibleDate(value: string): string { return value.slice(0, 10) }
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section class="section shell" :lang="locale">
      <div v-if="categories.length" class="filter-panel">
        <label>
          <span>{{ t('content.category') }}</span>
          <select v-model="category">
            <option value="all">{{ t('content.allCategories') }}</option>
            <option v-for="item in categories" :key="item" :value="item">{{ item }}</option>
          </select>
        </label>
      </div>
      <div v-if="entries.length" class="card-grid collection-grid">
        <article v-for="entry in entries" :key="entry.href" class="card collection-card" lang="en">
          <p v-if="entry.category" class="eyebrow">{{ entry.category }}</p>
          <h2><a :href="entry.href">{{ entry.title }}</a></h2>
          <p v-if="entry.summary">{{ entry.summary }}</p>
          <p v-if="entry.author || entry.publishedAt" class="published-content-meta">
            <span v-if="entry.author">{{ entry.author }}</span>
            <time v-if="entry.publishedAt" :datetime="entry.publishedAt">{{ visibleDate(entry.publishedAt) }}</time>
          </p>
        </article>
      </div>
      <div v-else class="empty-state" role="status">{{ t('content.noNews') }}</div>
      <p v-if="pageError" class="form-error" role="alert">{{ pageError }}</p>
      <nav v-if="pageIndex > 0 || pageNextCursor" class="catalog-pagination" :aria-label="t('content.newsPages')">
        <button class="button secondary" type="button" :disabled="pageIndex === 0 || loading" @click="previousPage">{{ t('common.previousPage') }}</button>
        <span aria-live="polite">{{ t('common.page', { page: pageIndex + 1 }) }}</span>
        <button class="button secondary" type="button" :disabled="!pageNextCursor || loading" @click="nextPage">{{ t('common.nextPage') }}</button>
      </nav>
      <p v-if="loading" class="catalog-loading" role="status">{{ t('content.loadingNews') }}</p>
    </section>
  </main>
</template>
