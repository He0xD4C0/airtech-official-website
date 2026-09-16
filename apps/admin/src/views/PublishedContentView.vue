<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { CmsPublishedContent } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { contentApi } from '@/services/contentApi'

const items = ref<CmsPublishedContent[]>([])
const state = ref<'loading' | 'ready' | 'empty' | 'error'>('loading')
const error = ref('')

async function load(): Promise<void> {
  try {
    items.value = (await contentApi.listPublished({ limit: 100 })).items
    state.value = items.value.length ? 'ready' : 'empty'
  } catch (reason) {
    state.value = 'error'
    error.value = reason instanceof Error ? reason.message : '已发布内容加载失败。'
  }
}
onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / PUBLISHED" title="公司已发布内容" description="每个 contentId 仅保留当前发布正文。">
      <template #actions><RouterLink class="button button--quiet" to="/content/drafts">私人草稿</RouterLink></template>
    </PageHeader>
    <DataStatePanel v-if="state !== 'ready'" :state="state" :title="state === 'empty' ? '尚无发布内容' : error" @retry="load" />
    <section v-else class="panel published-list">
      <RouterLink v-for="item in items" :key="item.contentId" :to="`/content/published/${item.contentId}`">
        <strong>{{ item.document.title }}</strong>
        <span>{{ item.document.kind }} · publication {{ item.publicationVersion }}</span>
      </RouterLink>
    </section>
  </div>
</template>

<style scoped>
.published-list { display: flex; flex-direction: column; }.published-list a { padding: .75rem; color: inherit; text-decoration: none; border-bottom: 1px solid var(--color-border); }.published-list strong,.published-list span { display: block; }.published-list span { color: var(--admin-muted); font-size: .76rem; }
</style>
