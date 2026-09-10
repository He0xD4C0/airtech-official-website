<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowDownUp, FilePlus2, Filter, Plus, Search, X } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import type { ContentRecordV2 } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { contentKindLabel, contentStatusLabels } from '@/components/content/labels'
import { contentApi, type ContentSortField } from '@/services/contentApi'
import {
  buildContentListRouteQuery,
  parseContentListQuery,
  toggleKindSelection,
  type ContentListFilters,
} from '@/services/contentListQuery'

type PageState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'
type StatusFilter = 'draft' | 'published' | 'archived' | undefined

const route = useRoute()
const router = useRouter()

const items = ref<ContentRecordV2[]>([])
const counts = ref<Record<string, number>>({})
const total = ref(0)
const nextCursor = ref<string | null>(null)
const cursorStack = ref<string[]>([])
const pageState = ref<PageState>('loading')
const errorMessage = ref('')
const searchInput = ref('')
let searchTimer: ReturnType<typeof setTimeout> | undefined

const query = computed(() => parseContentListQuery(route.query as Record<string, unknown>))

const hasFilters = computed(() => Boolean(query.value.q || query.value.kinds.length || query.value.status))
const displayTotal = computed(() => {
  if (!query.value.kinds.length) return total.value
  return query.value.kinds.reduce((sum, kind) => sum + (counts.value[kind] ?? 0), 0)
})
const kindTabs = computed(() => {
  const entries = new Map(
    Object.entries(counts.value).filter(([, count]) => count > 0),
  )
  for (const kind of query.value.kinds) {
    if (!entries.has(kind)) entries.set(kind, counts.value[kind] ?? 0)
  }
  return [...entries.entries()]
    .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
})
const showKind = (kind: string): boolean => query.value.kinds.includes(kind)

async function applyQuery(patch: Partial<ContentListFilters>): Promise<void> {
  await router.replace({ query: buildContentListRouteQuery(query.value, patch) })
}

function toggleKind(kind: string): void {
  void applyQuery({ kinds: toggleKindSelection(query.value.kinds, kind) })
}

async function load(reset: boolean): Promise<void> {
  if (reset) {
    cursorStack.value = []
    nextCursor.value = null
    pageState.value = 'loading'
  }
  try {
    const page = await contentApi.listContent({
      q: query.value.q || undefined,
      kinds: query.value.kinds.length ? query.value.kinds : undefined,
      status: query.value.status,
      sort: query.value.sort,
      direction: query.value.direction,
      cursor: cursorStack.value.at(-1),
      limit: 50,
    })
    items.value = page.items
    counts.value = page.counts
    total.value = page.total
    nextCursor.value = page.nextCursor
    pageState.value = page.items.length ? 'ready' : 'empty'
    errorMessage.value = ''
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) {
      pageState.value = 'forbidden'
    } else {
      pageState.value = 'error'
      errorMessage.value = error instanceof Error ? error.message : '内容列表加载失败。'
    }
  }
}

async function nextPage(): Promise<void> {
  if (!nextCursor.value) return
  cursorStack.value.push(nextCursor.value)
  await load(false)
}

async function previousPage(): Promise<void> {
  if (!cursorStack.value.length) return
  cursorStack.value.pop()
  await load(false)
}

function onSearchInput(): void {
  if (searchTimer !== undefined) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    void applyQuery({ q: searchInput.value })
  }, 300)
}

function clearSearch(): void {
  searchInput.value = ''
  void applyQuery({ q: '' })
}

function badgeTone(status: string): 'neutral' | 'success' | 'warning' {
  if (status === 'published') return 'success'
  if (status === 'archived') return 'warning'
  return 'neutral'
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

watch(
  () => [route.query.q, route.query.kind, route.query.status, route.query.sort, route.query.direction].join('|'),
  () => {
    searchInput.value = query.value.q
    void load(true)
  },
)

onMounted(() => {
  searchInput.value = query.value.q
  void load(true)
})

onBeforeUnmount(() => {
  if (searchTimer !== undefined) clearTimeout(searchTimer)
})
</script>

<template>
  <div class="page-stack">
    <PageHeader
      eyebrow="CONTENT"
      :title="query.kinds.includes('news') ? '新闻内容' : '内容中心'"
      description="服务端搜索、筛选、排序与类型计数；News 与其它内容共用同一套受控模板编辑器。"
    >
      <template #actions>
        <RouterLink class="button button--primary" to="/content/new"><FilePlus2 :size="16" />新建内容</RouterLink>
      </template>
    </PageHeader>

    <section class="panel content-list-toolbar" aria-label="内容筛选">
      <label class="search-field">
        <Search :size="17" />
        <span class="sr-only">搜索内容</span>
        <input
          v-model="searchInput"
          type="search"
          placeholder="按标题或 slug 搜索（服务端）"
          @input="onSearchInput"
        />
      </label>
      <button v-if="searchInput" class="icon-button" type="button" aria-label="清除搜索" @click="clearSearch"><X :size="16" /></button>

      <div class="content-list-toolbar__filters">
        <label>
          <Filter :size="15" />
          <span class="sr-only">状态筛选</span>
          <select :value="query.status ?? ''" @change="applyQuery({ status: (($event.target as HTMLSelectElement).value || undefined) as StatusFilter })">
            <option value="">全部状态</option>
            <option value="draft">草稿</option>
            <option value="published">已发布</option>
            <option value="archived">已归档</option>
          </select>
        </label>
        <label>
          <ArrowDownUp :size="15" />
          <span class="sr-only">排序字段</span>
          <select :value="query.sort" @change="applyQuery({ sort: ($event.target as HTMLSelectElement).value as ContentSortField, direction: ($event.target as HTMLSelectElement).value === 'updatedAt' ? 'desc' : 'asc' })">
            <option value="updatedAt">最近更新</option>
            <option value="title">标题</option>
            <option value="kind">类型</option>
          </select>
        </label>
        <button
          class="button button--quiet"
          type="button"
          :aria-label="query.direction === 'asc' ? '当前升序，点击切换降序' : '当前降序，点击切换升序'"
          @click="applyQuery({ direction: query.direction === 'asc' ? 'desc' : 'asc' })"
        >{{ query.direction === 'asc' ? '升序 ↑' : '降序 ↓' }}</button>
      </div>
    </section>

    <section class="content-type-strip" aria-label="内容类型计数">
      <button type="button" :aria-pressed="query.kinds.length === 0" :class="{ 'is-active': query.kinds.length === 0 }" @click="applyQuery({ kinds: [] })">
        <span>全部内容<strong>{{ total }}</strong></span>
      </button>
      <button
        v-for="[kind, count] in kindTabs"
        :key="kind"
        type="button"
        :aria-pressed="showKind(kind)"
        :class="{ 'is-active': showKind(kind) }"
        @click="toggleKind(kind)"
      >
        <span>{{ contentKindLabel(kind) }}<strong>{{ count }}</strong></span>
      </button>
      <RouterLink class="content-type-strip__link" to="/content?kind=news"><Plus :size="14" />News 视图</RouterLink>
    </section>

    <DataStatePanel
      v-if="pageState !== 'ready'"
      :state="pageState === 'loading' ? 'loading' : pageState === 'forbidden' ? 'forbidden' : pageState === 'error' ? 'error' : 'empty'"
      :title="pageState === 'empty' ? (hasFilters ? '没有匹配筛选条件的内容' : '数据库中暂无内容记录') : pageState === 'error' ? errorMessage : ''"
      @retry="load(true)"
    />

    <section v-else class="panel table-panel">
      <div class="data-table-wrap">
        <table class="data-table">
          <caption class="sr-only">内容列表，共 {{ displayTotal }} 条记录</caption>
          <thead>
            <tr>
              <th scope="col">内容</th>
              <th scope="col">类型</th>
              <th scope="col">语言</th>
              <th scope="col">状态</th>
              <th scope="col">最近更新</th>
              <th scope="col">更新人</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="entry in items" :key="entry.id">
              <td>
                <RouterLink :to="`/content/${entry.id}/edit`">
                  <strong>{{ entry.draft.title || '未命名内容' }}</strong>
                  <span>
                    {{ entry.draft.templateKey }}
                    <template v-if="entry.draft.isPlaceholder"> · 占位内容 / noindex</template>
                    <template v-else-if="entry.publishedRevision"> · Revision {{ entry.publishedRevision }}</template>
                  </span>
                </RouterLink>
              </td>
              <td>{{ contentKindLabel(entry.draft.kind) }}</td>
              <td><span class="locale-chip">{{ entry.draft.locale.toUpperCase() }}</span></td>
              <td><StatusBadge :label="contentStatusLabels[entry.status] ?? entry.status" :tone="badgeTone(entry.status)" /></td>
              <td>{{ formatDate(entry.updatedAt) }}</td>
              <td>{{ entry.updatedBy }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="content-list-pagination" aria-label="分页">
        <span>共 {{ displayTotal }} 条 · 本页 {{ items.length }} 条</span>
        <div>
          <button class="button button--quiet" type="button" :disabled="!cursorStack.length" @click="previousPage">上一页</button>
          <button class="button button--quiet" type="button" :disabled="!nextCursor" @click="nextPage">下一页</button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.content-list-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: .6rem; }
.content-list-toolbar__filters { display: flex; align-items: center; gap: .5rem; margin-left: auto; }
.content-list-toolbar__filters label { display: inline-flex; align-items: center; gap: .35rem; }
.content-list-toolbar select { padding: .35rem .5rem; }
.content-type-strip__link { display: inline-flex; align-items: center; gap: .3rem; margin-left: auto; font-size: .82rem; }
.content-list-pagination { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .6rem .25rem 0; font-size: .82rem; }
.content-list-pagination div { display: flex; gap: .4rem; }
</style>
