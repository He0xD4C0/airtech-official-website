<script setup lang="ts">
import { computed, ref } from 'vue'
import type { PublicSearchItem, PublicSearchType } from '@airtek/contracts'
import { PublicBlockRenderer } from '@/features/content'
import { useQueryHistory } from '@/shared/lib/queryHistory'
import { trackAnalyticsEvent } from '@/features/analytics'
import { searchPublishedSite } from '@/shared/lib/api'
import type { PublicPageModel } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
const query = ref(props.page.searchState?.q ?? '')
const type = ref<PublicSearchType | 'all'>(props.page.searchState?.type ?? 'all')
const items = ref<PublicSearchItem[]>([...(props.page.searchResults ?? [])])
const nextCursor = ref<string | null>(props.page.searchNextCursor ?? null)
const total = ref(props.page.searchTotal ?? 0)
const typeCounts = ref([...(props.page.searchTypeCounts ?? [])])
const cursorHistory = ref<Array<string | null>>([props.page.searchState?.cursor ?? null])
const pageIndex = ref(0)
const loading = ref(false)
const error = ref('')
let requestGeneration = 0

const availableTypes = computed(() => typeCounts.value.map((facet) => ({
  value: facet.value as PublicSearchType,
  label: t(`search.types.${facet.value as PublicSearchType}`),
  count: facet.count,
})))

const writeQuery = useQueryHistory((params, cursors) => {
  query.value = params.get('q') ?? ''
  const requestedType = params.get('type')
  type.value = requestedType && ['product', 'solution', 'technology', 'article', 'news', 'faq', 'caseStudy', 'download', 'company', 'page'].includes(requestedType) ? requestedType as PublicSearchType : 'all'
  cursorHistory.value = cursors
  pageIndex.value = Math.max(0, cursors.indexOf(params.get('cursor')))
  void loadPage(params.get('cursor'))
})

function syncUrl(): void {
  writeQuery({
    q: query.value.trim(),
    type: type.value === 'all' ? undefined : type.value,
    cursor: cursorHistory.value[pageIndex.value] ?? undefined,
  }, cursorHistory.value)
}

function requestQuery(cursor: string | null) {
  return {
    limit: 20,
    ...(query.value.trim() ? { q: query.value.trim() } : {}),
    ...(type.value !== 'all' ? { type: type.value } : {}),
    ...(cursor ? { cursor } : {}),
  }
}

async function loadPage(cursor: string | null): Promise<boolean> {
  const generation = ++requestGeneration
  loading.value = true
  error.value = ''
  try {
    const page = await searchPublishedSite(requestQuery(cursor))
    if (generation !== requestGeneration) return false
    items.value = page.items
    nextCursor.value = page.nextCursor
    total.value = page.total
    typeCounts.value = page.typeCounts
    return true
  } catch {
    if (generation === requestGeneration) {
      error.value = t('errors.search')
    }
    return false
  } finally {
    if (generation === requestGeneration) loading.value = false
  }
}

async function reset(kind: 'query' | 'type'): Promise<void> {
  cursorHistory.value = [null]
  pageIndex.value = 0
  nextCursor.value = null
  syncUrl()
  await loadPage(null)
  if (kind === 'query' && query.value.trim()) {
    void trackAnalyticsEvent('internalSearch', { queryLength: query.value.trim().length, resultCount: total.value })
  } else if (kind === 'type') {
    void trackAnalyticsEvent('filterApplied', { filterName: 'contentType', resultCount: total.value })
  }
}

async function nextPage(): Promise<void> {
  const cursor = nextCursor.value
  if (!cursor || loading.value) return
  if (await loadPage(cursor)) {
    cursorHistory.value = [...cursorHistory.value.slice(0, pageIndex.value + 1), cursor]
    pageIndex.value += 1
    syncUrl()
  }
}

async function previousPage(): Promise<void> {
  if (!pageIndex.value || loading.value) return
  const target = pageIndex.value - 1
  if (await loadPage(cursorHistory.value[target] ?? null)) {
    pageIndex.value = target
    syncUrl()
  }
}
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section class="section shell search-layout" :lang="locale">
      <div class="search-controls" :aria-busy="loading">
        <label><span>{{ t('search.label') }}</span><input v-model="query" autofocus type="search" :placeholder="t('search.placeholder')" @change="reset('query')" @keydown.enter.prevent="reset('query')"></label>
        <label><span>{{ t('search.contentType') }}</span><select v-model="type" @change="reset('type')">
          <option value="all">{{ t('search.allContent') }}</option>
          <option v-for="entryType in availableTypes" :key="entryType.value" :value="entryType.value">{{ entryType.label }} ({{ entryType.count }})</option>
        </select></label>
      </div>
      <p class="result-count" role="status">{{ t('search.results', { count: total }, total) }}</p>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div v-if="items.length" class="search-results">
        <article v-for="entry in items" :key="`${entry.entityType}:${entry.entityId}`">
          <span>{{ t(`search.types.${entry.displayType}`) }}</span>
          <h2 lang="en"><a :href="entry.canonicalPath">{{ entry.title }}</a></h2>
          <p lang="en">{{ entry.summary || t('search.summaryMissing') }}</p>
        </article>
      </div>
      <div v-else class="empty-state"><h2>{{ t('search.noMatching') }}</h2><p>{{ t('search.noMatchingDescription') }}</p></div>
      <nav v-if="pageIndex > 0 || nextCursor" class="catalog-pagination" :aria-label="t('search.pages')">
        <button class="button secondary" type="button" :disabled="pageIndex === 0 || loading" @click="previousPage">{{ t('common.previousPage') }}</button>
        <span>{{ t('common.page', { page: pageIndex + 1 }) }}<small>{{ t('search.pageCount', { shown: items.length, total }) }}</small></span>
        <button class="button secondary" type="button" :disabled="!nextCursor || loading" @click="nextPage">{{ t('common.nextPage') }}</button>
      </nav>
      <p v-if="loading" role="status">{{ t('search.loading') }}</p>
    </section>
  </main>
</template>
