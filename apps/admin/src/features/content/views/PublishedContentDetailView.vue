<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { CmsPublishedContent } from '@airtek/contracts'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import PublishedDocumentView from '@/features/content/components/PublishedDocumentView.vue'
import { contentApi } from '@/features/content/services/contentApi'

const route = useRoute()
const router = useRouter()
const record = ref<CmsPublishedContent | null>(null)
const error = ref('')
const copying = ref(false)

async function load(): Promise<void> {
  try { record.value = await contentApi.getPublished(String(route.params.contentId)) }
  catch (reason) { error.value = reason instanceof Error ? reason.message : '已发布内容加载失败。' }
}

async function copy(): Promise<void> {
  if (!record.value || copying.value) return
  copying.value = true
  try {
    const draft = (await contentApi.copyPublished(record.value.contentId)).draft
    await router.push(`/content/drafts/${draft.draftId}`)
  } finally { copying.value = false }
}
onMounted(load)
</script>

<template>
  <div class="page-stack">
    <DataStatePanel v-if="!record" :state="error ? 'error' : 'loading'" :title="error" @retry="load" />
    <template v-else>
      <PageHeader eyebrow="CONTENT / PUBLISHED" :title="record.document.title" :description="`当前发布版本 ${record.publicationVersion}`">
        <template #actions><button class="button button--primary" :disabled="copying" @click="copy">{{ copying ? '正在复制…' : '复制为新私人草稿' }}</button></template>
      </PageHeader>
      <PublishedDocumentView :document="record.document" />
    </template>
  </div>
</template>
