<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { CmsReviewItem } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { contentApi } from '@/services/contentApi'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
const items = ref<CmsReviewItem[]>([])
const state = ref<'loading' | 'ready' | 'empty' | 'error'>('loading')
const error = ref('')

async function load(): Promise<void> {
  state.value = 'loading'
  try {
    items.value = (await contentApi.listReviews({ limit: 100 })).items
    state.value = items.value.length ? 'ready' : 'empty'
  } catch (reason) {
    state.value = 'error'
    error.value = reason instanceof Error ? reason.message : '审核队列加载失败。'
  }
}

async function approve(item: CmsReviewItem): Promise<void> {
  try {
    await contentApi.approve(item.draft.draftId)
    ui.toast('审核通过', '当前发布内容已覆盖，私人草稿已删除。')
    await load()
  } catch (reason) {
    ui.toast('审核失败', reason instanceof Error ? reason.message : '请重试。', 'danger')
  }
}

async function reject(item: CmsReviewItem): Promise<void> {
  const reason = window.prompt('请输入当前拒绝原因')?.trim()
  if (!reason) return
  try {
    await contentApi.reject(item.draft.draftId, reason)
    ui.toast('已拒绝', '草稿已恢复为编辑状态。')
    await load()
  } catch (failure) {
    ui.toast('拒绝失败', failure instanceof Error ? failure.message : '请重试。', 'danger')
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / REVIEWS" title="内容审核" description="待审核草稿在此冻结；通过后原子覆盖当前发布内容。" />
    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '审核队列为空' : error" @retry="load" />
    <section v-else class="panel review-list">
      <article v-for="item in items" :key="item.draft.draftId" class="review-row">
        <div>
          <strong>{{ item.draft.document.title }}</strong>
          <span>基础发布版本 {{ item.draft.basePublicationVersion }} · 草稿版本 {{ item.draft.draftVersion }}</span>
        </div>
        <RouterLink class="button button--quiet" :to="`/content/drafts/${item.draft.draftId}`">查看</RouterLink>
        <button class="button button--quiet" @click="reject(item)">拒绝</button>
        <button class="button button--primary" @click="approve(item)">通过并发布</button>
      </article>
    </section>
  </div>
</template>

<style scoped>
.review-list { display: flex; flex-direction: column; gap: .4rem; }
.review-row { display: flex; align-items: center; gap: .5rem; padding: .7rem; border-bottom: 1px solid var(--color-border); }
.review-row div { flex: 1; }.review-row strong,.review-row span { display: block; }.review-row span { color: var(--admin-muted); font-size: .76rem; }
</style>
