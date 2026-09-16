<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { CmsPrivateDraft } from '@airtek/contracts'
import { FilePlus2, Search } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { contentApi } from '@/services/contentApi'
import { useAuthStore } from '@/stores/auth'

const items = ref<CmsPrivateDraft[]>([])
const nextCursor = ref<string | null>(null)
const cursorStack = ref<string[]>([])
const total = ref(0)
const search = ref('')
const state = ref<'loading' | 'ready' | 'empty' | 'error'>('loading')
const error = ref('')
const auth = useAuthStore()

async function load(reset = false): Promise<void> {
  if (reset) cursorStack.value = []
  state.value = 'loading'
  try {
    const page = await contentApi.listDrafts({
      q: search.value || undefined,
      cursor: cursorStack.value.at(-1),
      limit: 50,
    })
    items.value = page.items
    nextCursor.value = page.nextCursor
    total.value = page.total
    state.value = page.items.length ? 'ready' : 'empty'
  } catch (reason) {
    state.value = 'error'
    error.value = reason instanceof Error ? reason.message : '私人草稿加载失败。'
  }
}

async function next(): Promise<void> {
  if (!nextCursor.value) return
  cursorStack.value.push(nextCursor.value)
  await load()
}

async function previous(): Promise<void> {
  cursorStack.value.pop()
  await load()
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'medium', timeStyle: 'short' })
    .format(new Date(value))
}

async function claim(item: CmsPrivateDraft): Promise<void> {
  await contentApi.claimDraft(item.draftId)
  await load()
}

onMounted(() => load(true))
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / DRAFTS" title="私人草稿" description="仅显示自己的、分享给自己的和管理链下级草稿。">
      <template #actions>
        <RouterLink class="button button--quiet" to="/content/published">公司已发布内容</RouterLink>
        <RouterLink class="button button--primary" to="/content/drafts/new"><FilePlus2 :size="16" />新建私人草稿</RouterLink>
      </template>
    </PageHeader>

    <form class="panel draft-search" @submit.prevent="load(true)">
      <Search :size="16" />
      <input v-model="search" type="search" placeholder="按标题或 slug 搜索" />
      <button class="button button--quiet" type="submit">搜索</button>
    </form>

    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? '没有可见私人草稿' : state === 'error' ? error : ''"
      @retry="load()"
    />

    <section v-else class="panel draft-list">
      <div class="draft-list__summary">共 {{ total }} 份当前草稿</div>
      <div v-for="item in items" :key="item.draftId" class="draft-row">
        <RouterLink :to="`/content/drafts/${item.draftId}`">
          <strong>{{ item.document.title || '未命名草稿' }}</strong>
          <span>{{ item.document.kind }} · {{ item.document.slug || '无公开路径' }}</span>
        </RouterLink>
        <StatusBadge :label="item.state === 'pendingReview' ? '待审核' : '编辑中'" :tone="item.state === 'pendingReview' ? 'warning' : 'neutral'" />
        <button v-if="!item.ownerUserId && auth.user?.role === 'super-admin'" class="button button--quiet" type="button" @click="claim(item)">认领</button>
        <time :datetime="item.updatedAt">{{ formatDate(item.updatedAt) }}</time>
      </div>
      <div class="draft-list__paging">
        <button class="button button--quiet" :disabled="!cursorStack.length" @click="previous">上一页</button>
        <button class="button button--quiet" :disabled="!nextCursor" @click="next">下一页</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.draft-search { display: flex; align-items: center; gap: .5rem; }
.draft-search input { flex: 1; }
.draft-list { display: flex; flex-direction: column; gap: .25rem; }
.draft-list__summary { color: var(--admin-muted); font-size: .8rem; padding: .4rem; }
.draft-row { display: grid; grid-template-columns: minmax(0,1fr) auto auto auto; align-items: center; gap: 1rem; padding: .75rem; border-radius: .45rem; color: inherit; }
.draft-row a { color: inherit; text-decoration: none; }
.draft-row:hover { background: #f5f8f8; }
.draft-row strong,.draft-row span { display: block; }
.draft-row span,.draft-row time { color: var(--admin-muted); font-size: .75rem; }
.draft-list__paging { display: flex; justify-content: flex-end; gap: .4rem; padding-top: .6rem; }
</style>
