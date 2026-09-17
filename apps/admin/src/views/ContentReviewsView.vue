<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { CmsReviewItem } from '@airtek/contracts'
import AdminDialog from '@/components/AdminDialog.vue'
import CursorPaginationControls from '@/components/CursorPaginationControls.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { useCursorPagination } from '@/composables/useCursorPagination'
import { contentApi } from '@/services/contentApi'
import { useUiStore } from '@/stores/ui'

type ReviewDecision = 'approve' | 'reject'
const ui = useUiStore()
const query = ref('')
const submittedQuery = ref('')
const total = ref(0)
const selected = ref<CmsReviewItem | null>(null)
const decision = ref<ReviewDecision>('approve')
const rejectReason = ref('')
const deciding = ref(false)
const pager = useCursorPagination(async (pagination) => {
  const page = await contentApi.listReviews({
    cursor: pagination.cursor ?? undefined,
    limit: pagination.limit,
    q: submittedQuery.value || undefined,
  })
  total.value = page.total
  return page
}, { errorMessage: '审核队列加载失败。' })
const state = computed(() => {
  if (pager.loading.value && !pager.items.value.length) return 'loading'
  if (pager.error.value) return 'error'
  return pager.items.value.length ? 'ready' : 'empty'
})
const dialogOpen = computed({
  get: () => Boolean(selected.value),
  set: (value: boolean) => { if (!value) selected.value = null },
})

async function search(): Promise<void> {
  submittedQuery.value = query.value.trim()
  await pager.first()
}

function openDecision(item: CmsReviewItem, next: ReviewDecision): void {
  selected.value = item
  decision.value = next
  rejectReason.value = ''
}

async function completeDecision(): Promise<void> {
  if (!selected.value || deciding.value) return
  const reason = rejectReason.value.trim()
  if (decision.value === 'reject' && !reason) return
  deciding.value = true
  try {
    if (decision.value === 'approve') {
      await contentApi.approve(selected.value.draft.draftId)
      ui.toast('审核通过', '当前发布内容已覆盖，私人草稿已删除。')
    } else {
      await contentApi.reject(selected.value.draft.draftId, reason)
      ui.toast('已拒绝', '草稿已恢复为编辑状态。')
    }
    selected.value = null
    if (pager.items.value.length === 1 && pager.canPrevious.value) await pager.previous()
    else await pager.refresh()
  } catch (failure) {
    ui.toast('审核失败', failure instanceof Error ? failure.message : '请重试。', 'danger')
  } finally {
    deciding.value = false
  }
}

onMounted(pager.first)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / REVIEWS" title="内容审核" description="待审核草稿在此冻结；通过后原子覆盖当前发布内容。" />
    <form class="panel draft-search" role="search" @submit.prevent="search">
      <label class="field">
        <span>搜索待审核内容</span>
        <input v-model="query" type="search" maxlength="200" placeholder="标题或 Slug" />
      </label>
      <button class="button button--quiet" type="submit" :disabled="pager.loading.value">搜索</button>
    </form>
    <DataStatePanel
      v-if="state !== 'ready'"
      :state="state"
      :title="state === 'empty' ? (submittedQuery ? '没有匹配的待审核内容' : '审核队列为空') : pager.error.value || ''"
      @retry="pager.refresh"
    />
    <template v-else>
      <section class="panel review-list">
        <article v-for="item in pager.items.value" :key="item.draft.draftId" class="review-row">
          <div>
            <strong>{{ item.draft.document.title }}</strong>
            <span>基础发布版本 {{ item.draft.basePublicationVersion }} · 草稿版本 {{ item.draft.draftVersion }}</span>
          </div>
          <RouterLink class="button button--quiet" :to="`/content/drafts/${item.draft.draftId}`">查看</RouterLink>
          <button class="button button--quiet" type="button" @click="openDecision(item, 'reject')">拒绝</button>
          <button class="button button--primary" type="button" @click="openDecision(item, 'approve')">通过并发布</button>
        </article>
      </section>
      <CursorPaginationControls
        :item-count="pager.items.value.length"
        :page-number="pager.pageNumber.value"
        :can-previous="pager.canPrevious.value"
        :can-next="pager.canNext.value"
        :loading="pager.loading.value"
        :label="`条待审核内容，共 ${total} 条`"
        @previous="pager.previous"
        @next="pager.next"
      />
    </template>

    <AdminDialog v-model="dialogOpen" :title="decision === 'approve' ? '确认通过并发布' : '拒绝并退回编辑'">
      <p v-if="selected">{{ selected.draft.document.title }}</p>
      <p v-if="decision === 'approve'">此操作会原子覆盖当前发布版本，并删除对应私人草稿。</p>
      <label v-else class="field">
        <span>拒绝理由</span>
        <textarea v-model="rejectReason" autofocus required minlength="1" maxlength="2000" rows="5" />
        <small>{{ rejectReason.trim().length }} / 2000</small>
      </label>
      <template #actions>
        <button class="button button--quiet" type="button" :disabled="deciding" @click="dialogOpen = false">取消</button>
        <button
          :class="['button', decision === 'approve' ? 'button--primary' : 'button--danger']"
          type="button"
          :disabled="deciding || (decision === 'reject' && !rejectReason.trim())"
          @click="completeDecision"
        >{{ deciding ? '正在处理…' : decision === 'approve' ? '确认发布' : '确认拒绝' }}</button>
      </template>
    </AdminDialog>
  </div>
</template>

<style scoped>
.review-list { display: flex; flex-direction: column; gap: .4rem; }
.review-row { display: flex; align-items: center; gap: .5rem; padding: .7rem; border-bottom: 1px solid var(--admin-line); }
.review-row div { flex: 1; }
.review-row strong, .review-row span { display: block; }
.review-row span { color: var(--admin-muted); font-size: .76rem; }
.button--danger { border-color: #a72f24; background: #a72f24; color: white; }
</style>
