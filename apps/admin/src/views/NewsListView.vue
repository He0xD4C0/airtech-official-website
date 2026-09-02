<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { Newspaper, Plus, RefreshCw } from 'lucide-vue-next'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { adminApi, type BackendNewsEntry } from '@/services/adminApi'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

const auth = useAuthStore()
const ui = useUiStore()
const newsPager = useCursorPagination<BackendNewsEntry>((pagination) => adminApi.listNews(pagination), {
  errorMessage: 'News 列表读取失败。',
  onError: (message) => ui.toast('News 读取失败', message, 'danger'),
})
const items = computed(() => newsPager.items.value)
const state = computed<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(() => {
  if (newsPager.loading.value && !items.value.length) return 'loading'
  if (newsPager.errorStatus.value === 403) return 'forbidden'
  if (newsPager.error.value) return 'error'
  return items.value.length ? 'ready' : 'empty'
})

onMounted(newsPager.first)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="NEWS" title="新闻中心" description="News 是独立内容类型；正文、SEO 与发布 revision 均来自 PostgreSQL。">
      <template #actions>
        <button class="button button--secondary" type="button" :disabled="newsPager.loading.value" @click="newsPager.refresh"><RefreshCw :size="16" />刷新</button>
        <RouterLink v-if="auth.hasPermission('content.write')" class="button button--primary" to="/news/new"><Plus :size="16" />新建 News</RouterLink>
      </template>
    </PageHeader>

    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '暂无 News' : ''" :description="state === 'empty' ? '创建 News 后可预览、发布、回滚，并进入公开 News 路由。' : ''" @retry="newsPager.refresh" />

    <section v-else class="panel table-panel">
      <div class="data-table-wrap">
        <table class="data-table">
          <thead><tr><th>标题</th><th>分类</th><th>发布日期</th><th>Revision</th><th>索引</th><th>状态</th></tr></thead>
          <tbody>
            <tr v-for="item in items" :key="item.content.id">
              <td><RouterLink v-if="auth.hasPermission('content.write')" class="entity-link" :to="`/news/${item.content.id}/edit`"><Newspaper :size="16" /><span><strong>{{ item.content.title }}</strong><small>/{{ item.content.slug }}</small></span></RouterLink><span v-else class="entity-link"><Newspaper :size="16" /><span><strong>{{ item.content.title }}</strong><small>/{{ item.content.slug }}</small></span></span></td>
              <td>{{ item.category || '未分类' }}</td>
              <td>{{ item.publishedAt ? new Date(item.publishedAt).toLocaleDateString('zh-CN') : '未设置' }}</td>
              <td>{{ item.content.publishedRevision ?? '—' }} / {{ item.content.currentRevision }}</td>
              <td><StatusBadge :label="item.content.seo.indexable && !item.content.isPlaceholder ? 'index' : 'noindex'" :tone="item.content.seo.indexable && !item.content.isPlaceholder ? 'success' : 'warning'" /></td>
              <td><StatusBadge :label="item.content.status" :tone="item.content.status === 'published' ? 'success' : 'neutral'" /></td>
            </tr>
          </tbody>
        </table>
      </div>
      <CursorPaginationControls
        :item-count="items.length"
        :page-number="newsPager.pageNumber.value"
        :can-previous="newsPager.canPrevious.value"
        :can-next="newsPager.canNext.value"
        :loading="newsPager.loading.value"
        label="条 News 记录"
        @previous="newsPager.previous"
        @next="newsPager.next"
      />
    </section>
  </div>
</template>
