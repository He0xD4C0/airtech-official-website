<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { contentApi } from '@/services/contentApi'

const query = ref('')
const submittedQuery = ref('')
const total = ref(0)
const pager = useCursorPagination(async (pagination) => {
  const page = await contentApi.listPublished({
    cursor: pagination.cursor ?? undefined,
    limit: pagination.limit,
    q: submittedQuery.value || undefined,
  })
  total.value = page.total
  return page
}, { errorMessage: '已发布内容加载失败。' })
const state = computed(() => {
  if (pager.loading.value && !pager.items.value.length) return 'loading'
  if (pager.error.value) return 'error'
  return pager.items.value.length ? 'ready' : 'empty'
})

async function search(): Promise<void> {
  submittedQuery.value = query.value.trim()
  await pager.first()
}

onMounted(pager.first)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / PUBLISHED" title="公司已发布内容" description="按标题或 Slug 搜索当前发布版本；列表由服务端分页。">
      <template #actions><RouterLink class="button button--quiet" to="/content/drafts">私人草稿</RouterLink></template>
    </PageHeader>
    <form class="panel draft-search" role="search" @submit.prevent="search">
      <label class="field">
        <span>搜索已发布内容</span>
        <input v-model="query" type="search" maxlength="200" placeholder="标题或 Slug" />
      </label>
      <button class="button button--quiet" type="submit" :disabled="pager.loading.value">搜索</button>
    </form>
    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? (submittedQuery ? '没有匹配的已发布内容' : '尚无发布内容') : pager.error.value || ''"
      @retry="pager.refresh"
    />
    <template v-else>
      <section class="panel published-list">
        <RouterLink v-for="item in pager.items.value" :key="item.contentId" :to="`/content/published/${item.contentId}`">
          <strong>{{ item.document.title }}</strong>
          <span>{{ item.document.kind }} · 发布版本 {{ item.publicationVersion }}</span>
        </RouterLink>
      </section>
      <CursorPaginationControls
        :item-count="pager.items.value.length"
        :page-number="pager.pageNumber.value"
        :can-previous="pager.canPrevious.value"
        :can-next="pager.canNext.value"
        :loading="pager.loading.value"
        :label="`条内容，共 ${total} 条`"
        @previous="pager.previous"
        @next="pager.next"
      />
    </template>
  </div>
</template>

<style scoped>
@layer components {
.published-list { display: flex; flex-direction: column; }
.published-list a { padding: .75rem; border-bottom: 1px solid var(--border-default); color: inherit; text-decoration: none; }
.published-list strong, .published-list span { display: block; }
.published-list span { color: var(--text-secondary); font-size: .76rem; }
}
</style>
