<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { BookOpenText, ChevronDown, FilePlus2, Filter, MoreHorizontal, Search, SlidersHorizontal } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, mockApiEnabled } from '@/services/adminApi'
import type { ContentEntry } from '@/types/domain'

const query = ref('')
const status = ref('all')

const demoEntries: ContentEntry[] = [
  { id: 'home', type: 'Home', title: 'Homepage', locale: 'EN', status: 'draft', updatedAt: '今天 09:42', updatedBy: 'Demo Editor', isPlaceholder: true },
  { id: 'products', type: 'Index', title: 'Products', locale: 'EN', status: 'draft', updatedAt: '昨天 17:08', updatedBy: 'Demo Editor', isPlaceholder: true },
  { id: 'solutions', type: 'Index', title: 'Solutions', locale: 'EN', status: 'scheduled', updatedAt: '昨天 15:21', updatedBy: 'Demo Publisher', isPlaceholder: true },
  { id: 'technology', type: 'Index', title: 'Technology', locale: 'EN', status: 'draft', updatedAt: '8 月 29 日', updatedBy: 'Demo Editor', isPlaceholder: true },
  { id: 'about', type: 'Company', title: 'About AIRTEKPOWER', locale: 'EN', status: 'draft', updatedAt: '8 月 28 日', updatedBy: 'Demo Editor', isPlaceholder: true },
  { id: 'contact', type: 'Company', title: 'Contact', locale: 'EN', status: 'archived', updatedAt: '8 月 26 日', updatedBy: 'Demo Publisher', isPlaceholder: true },
]

const contentPager = useCursorPagination(async (pagination) => {
  const page = await adminApi.listContent(pagination)
  return {
    ...page,
    items: page.items.map((entry): ContentEntry => ({
      id: entry.id,
      type: entry.kind,
      title: entry.title,
      locale: entry.locale.toUpperCase(),
      status: entry.status,
      updatedAt: new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(entry.updatedAt)),
      updatedBy: '列表响应未提供',
      publishedRevision: entry.publishedRevision?.toString(),
      isPlaceholder: entry.isPlaceholder,
    })),
  }
}, {
  errorMessage: '无法读取内容记录。',
})
const loadError = computed(() => contentPager.error.value ?? '')
const entries = computed(() => mockApiEnabled ? demoEntries : contentPager.items.value)

const filteredEntries = computed(() => entries.value.filter((entry) => {
  const matchesQuery = `${entry.title} ${entry.type}`.toLowerCase().includes(query.value.toLowerCase())
  const matchesStatus = status.value === 'all' || entry.status === status.value
  return matchesQuery && matchesStatus
}))
const typeCounts = computed(() => ({
  all: entries.value.length,
  pages: entries.value.filter((entry) => !['article', 'faq', 'caseStudy', 'download'].includes(entry.type)).length,
  articles: entries.value.filter((entry) => entry.type === 'article').length,
  faqs: entries.value.filter((entry) => entry.type === 'faq').length,
  cases: entries.value.filter((entry) => entry.type === 'caseStudy').length,
  downloads: entries.value.filter((entry) => entry.type === 'download').length,
}))

onMounted(async () => {
  if (mockApiEnabled) return
  await contentPager.first()
})

const badge = (entryStatus: ContentEntry['status']) => ({
  draft: { label: '草稿', tone: 'neutral' as const },
  scheduled: { label: '计划发布', tone: 'info' as const },
  published: { label: '已发布', tone: 'success' as const },
  archived: { label: '已归档', tone: 'warning' as const },
})[entryStatus]
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT" title="内容中心" description="统一管理公开站页面、文章、FAQ、案例、下载与全局内容。">
      <template #actions>
        <button class="button button--secondary" type="button"><SlidersHorizontal :size="16" />内容模型</button>
        <RouterLink class="button button--primary" to="/content/new/edit"><FilePlus2 :size="16" />新建内容</RouterLink>
      </template>
    </PageHeader>

    <div v-if="loadError" class="demo-banner"><span>API 错误</span><p>{{ loadError }}</p></div>

    <section class="content-type-strip" aria-label="内容类型">
      <button class="is-active" type="button"><BookOpenText :size="17" /><span>全部内容<strong>{{ typeCounts.all }}</strong></span></button>
      <button type="button"><span>页面<strong>{{ typeCounts.pages }}</strong></span></button>
      <button type="button"><span>Articles<strong>{{ typeCounts.articles }}</strong></span></button>
      <button type="button"><span>FAQ<strong>{{ typeCounts.faqs }}</strong></span></button>
      <button type="button"><span>Cases<strong>{{ typeCounts.cases }}</strong></span></button>
      <button type="button"><span>Downloads<strong>{{ typeCounts.downloads }}</strong></span></button>
    </section>

    <section class="panel table-panel">
      <div class="table-toolbar">
        <label class="search-field"><Search :size="17" /><input v-model="query" placeholder="搜索标题或类型" /></label>
        <div class="table-toolbar__filters">
          <Filter :size="16" />
          <select v-model="status" aria-label="按状态筛选"><option value="all">全部状态</option><option value="draft">草稿</option><option value="scheduled">计划发布</option><option value="published">已发布</option><option value="archived">已归档</option></select>
          <button type="button" class="button button--quiet">最近更新<ChevronDown :size="14" /></button>
        </div>
      </div>
      <div class="data-table-wrap">
        <table class="data-table">
          <thead><tr><th><input type="checkbox" aria-label="选择全部内容" /></th><th>内容</th><th>语言</th><th>状态</th><th>最近更新</th><th>更新人</th><th><span class="sr-only">操作</span></th></tr></thead>
          <tbody>
            <tr v-for="entry in filteredEntries" :key="entry.id">
              <td><input type="checkbox" :aria-label="`选择 ${entry.title}`" /></td>
              <td><RouterLink :to="`/content/${entry.id}/edit`"><strong>{{ entry.title }}</strong><span>{{ entry.type }} · <em v-if="entry.isPlaceholder">占位内容 / 不可索引</em><template v-else>Revision {{ entry.publishedRevision }}</template></span></RouterLink></td>
              <td><span class="locale-chip">{{ entry.locale }}</span></td>
              <td><StatusBadge v-bind="badge(entry.status)" /></td>
              <td>{{ entry.updatedAt }}</td>
              <td>{{ entry.updatedBy }}</td>
              <td><button class="icon-button" type="button" aria-label="更多操作"><MoreHorizontal :size="18" /></button></td>
            </tr>
          </tbody>
        </table>
      </div>
      <CursorPaginationControls
        :item-count="filteredEntries.length"
        :page-number="contentPager.pageNumber.value"
        :can-previous="!mockApiEnabled && contentPager.canPrevious.value"
        :can-next="!mockApiEnabled && contentPager.canNext.value"
        :loading="contentPager.loading.value"
        :label="mockApiEnabled ? '条开发演示记录' : '条数据库记录（筛选作用于当前页）'"
        @previous="contentPager.previous"
        @next="contentPager.next"
      />
    </section>
  </div>
</template>
