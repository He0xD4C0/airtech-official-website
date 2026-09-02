<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, Eye, RotateCcw, Save, Send } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import StructuredEditor from '@/components/StructuredEditor.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, type BackendNewsEntry, type NewsDraftPayload, type NewsRevision } from '@/services/adminApi'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { diffJsonRevisions } from '@/services/jsonRevisionDiff'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

interface EditorDocument { type: 'doc'; content?: Array<Record<string, unknown>> }

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const isNew = route.name === 'news-new'
const id = ref<string>()
const revision = ref<number>()
const title = ref('')
const slug = ref('')
const summary = ref('')
const category = ref('')
const authorDisplayName = ref('')
const coverMediaId = ref('')
const publishedAt = ref('')
const featured = ref(false)
const seoTitle = ref('')
const seoDescription = ref('')
const isPlaceholder = ref(true)
const indexable = ref(false)
const saving = ref(false)
const state = ref<'loading' | 'ready' | 'empty' | 'error' | 'forbidden'>(isNew ? 'ready' : 'loading')
const documentJson = ref<EditorDocument>({ type: 'doc', content: [] })
const revisions = ref<NewsRevision[]>([])
const selectedRevision = ref<number>()
const selectedSnapshot = computed(() => revisions.value.find((item) => item.content.currentRevision === selectedRevision.value))
const revisionDiff = computed(() => selectedSnapshot.value ? diffJsonRevisions(selectedSnapshot.value, { ...payload(), content: { ...payload().content, id: id.value, currentRevision: revision.value } }) : [])
let autosaveTimer: number | undefined

function displayJson(value: unknown): string {
  if (value === undefined) return '—'
  return typeof value === 'string' ? value : JSON.stringify(value)
}

async function loadRevisions(): Promise<void> {
  if (!id.value) {
    revisions.value = []
    selectedRevision.value = undefined
    return
  }
  revisions.value = await adminApi.listNewsRevisions(id.value)
  if (!revisions.value.some((item) => item.content.currentRevision === selectedRevision.value)) {
    selectedRevision.value = revisions.value[0]?.content.currentRevision
  }
}

function payload(): NewsDraftPayload {
  return {
    content: {
      kind: 'news',
      slug: slug.value.trim(),
      locale: 'en',
      title: title.value.trim(),
      summary: summary.value.trim() || null,
      body: { schemaVersion: 1, doc: { type: 'doc', content: documentJson.value.content ?? [] } },
      seo: {
        title: seoTitle.value.trim() || null,
        description: seoDescription.value.trim() || null,
        canonicalPath: `/en/resources/news/${slug.value.trim()}`,
        indexable: indexable.value && !isPlaceholder.value,
      },
      isPlaceholder: isPlaceholder.value,
    },
    category: category.value.trim(),
    authorDisplayName: authorDisplayName.value.trim(),
    coverMediaId: coverMediaId.value.trim() || null,
    publishedAt: publishedAt.value ? new Date(publishedAt.value).toISOString() : null,
    featured: featured.value,
    dataClass: isPlaceholder.value ? 'developmentFixture' : 'editorial',
  }
}

function documentRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

function applyEntry(entry: BackendNewsEntry): void {
  id.value = entry.content.id
  revision.value = entry.content.currentRevision
  title.value = entry.content.title
  slug.value = entry.content.slug
  summary.value = entry.content.summary ?? ''
  category.value = entry.category
  authorDisplayName.value = entry.authorDisplayName ?? ''
  coverMediaId.value = entry.coverMediaId ?? ''
  publishedAt.value = entry.publishedAt?.slice(0, 16) ?? ''
  featured.value = entry.featured
  seoTitle.value = entry.content.seo.title ?? ''
  seoDescription.value = entry.content.seo.description ?? ''
  isPlaceholder.value = entry.content.isPlaceholder
  indexable.value = entry.content.seo.indexable
  const doc = documentRecord(entry.content.body.doc)
  documentJson.value = {
    type: 'doc',
    content: Array.isArray(doc.content) ? doc.content as Array<Record<string, unknown>> : [],
  }
}

async function save(silent = false): Promise<boolean> {
  if (!title.value.trim() || !slug.value.trim()) {
    if (!silent) ui.toast('无法保存', '标题和 slug 为必填项。', 'warning')
    return false
  }
  saving.value = true
  try {
    const result = await adminApi.saveNews(payload(), id.value, revision.value)
    applyEntry(result.entry)
    if (!silent) await loadRevisions()
    if (!silent) ui.toast('News 草稿已保存', `Working revision ${result.entry.content.currentRevision}.`)
    return true
  } catch (error) {
    if (!silent) ui.toast('News 保存失败', apiErrorMessage(error, '请检查并发版本。'), 'danger')
    return false
  } finally {
    saving.value = false
  }
}

async function publish(): Promise<void> {
  if (!auth.hasPermission('content.publish')) {
    ui.toast('没有发布权限', '当前账号可以编辑 News 草稿，但不能发布。', 'warning')
    return
  }
  if (!await save(true) || !id.value || !revision.value) return
  try {
    const result = await adminApi.publishNews(id.value, revision.value)
    applyEntry(result.entry)
    await loadRevisions()
    ui.toast('News 已发布', isPlaceholder.value ? '占位 News 保持 noindex，且不会进入 Sitemap。' : '公开 projection 已更新。')
  } catch (error) {
    ui.toast('发布失败', apiErrorMessage(error, '请检查权限和 revision。'), 'danger')
  }
}

async function preview(): Promise<void> {
  if (!await save(true) || !id.value || !revision.value) return
  try {
    const link = await adminApi.createContentPreview(id.value, revision.value)
    window.open(link.url, '_blank', 'noopener,noreferrer')
  } catch (error) {
    ui.toast('预览失败', apiErrorMessage(error, '无法签发短效预览。'), 'danger')
  }
}

async function rollback(): Promise<void> {
  if (!auth.hasPermission('content.publish')) return
  if (!id.value || !revision.value || selectedRevision.value === undefined) return
  const targetRevision = selectedRevision.value
  const reason = window.prompt(`请输入回滚到 revision ${targetRevision} 的原因：`)?.trim()
  if (!reason) return
  if (reason.length < 10) {
    ui.toast('回滚原因过短', '审计原因至少需要 10 个字符。', 'warning')
    return
  }
  try {
    const result = await adminApi.rollbackNews(id.value, revision.value, targetRevision, reason)
    applyEntry(result.entry)
    await loadRevisions()
    ui.toast('已创建回滚 revision', '历史 revision 未被修改。')
  } catch (error) {
    ui.toast('回滚失败', apiErrorMessage(error, '请检查目标 revision。'), 'danger')
  }
}

watch([title, slug, summary, category, authorDisplayName, coverMediaId, publishedAt, featured, seoTitle, seoDescription, isPlaceholder, indexable, documentJson], () => {
  if (state.value !== 'ready' || !id.value) return
  window.clearTimeout(autosaveTimer)
  autosaveTimer = window.setTimeout(() => void save(true), 900)
}, { deep: true })

async function loadEntry(): Promise<void> {
  if (isNew) {
    state.value = 'ready'
    return
  }
  state.value = 'loading'
  try {
    applyEntry(await adminApi.getNews(String(route.params.id)))
    await loadRevisions()
    state.value = 'ready'
  } catch (error) {
    const status = apiProblemStatus(error)
    state.value = status === 403 ? 'forbidden' : status === 404 ? 'empty' : 'error'
    ui.toast('News 读取失败', apiErrorMessage(error, '记录不存在。'), 'danger')
  }
}

onMounted(async () => {
  await loadEntry()
  await nextTick()
})

onBeforeUnmount(() => window.clearTimeout(autosaveTimer))
</script>

<template>
  <div v-if="state !== 'ready'" class="page-stack">
    <DataStatePanel :state="state" @retry="loadEntry" />
  </div>
  <div v-else class="editor-page">
    <header class="editor-topbar">
      <div class="editor-topbar__left"><button class="icon-button" type="button" aria-label="返回 News" @click="router.push('/news')"><ArrowLeft :size="19" /></button><div><span>News · English</span><strong>{{ title || '未命名 News' }}</strong></div><StatusBadge :label="isPlaceholder ? 'Placeholder' : 'Editorial'" :tone="isPlaceholder ? 'warning' : 'success'" /></div>
      <div class="editor-topbar__right"><button class="button button--quiet" type="button" :disabled="saving" @click="save(false)"><Save :size="16" />保存</button><button class="button button--secondary" type="button" @click="preview"><Eye :size="16" />预览</button><button v-if="auth.hasPermission('content.publish')" class="button button--secondary" type="button" :disabled="selectedRevision === undefined" @click="rollback"><RotateCcw :size="16" />回滚所选</button><button v-if="auth.hasPermission('content.publish')" class="button button--primary" type="button" @click="publish"><Send :size="16" />发布</button></div>
    </header>
    <div class="editor-layout">
      <main class="editor-workspace">
        <div class="editor-title-fields"><label><span>标题</span><input v-model="title" /></label><label><span>摘要</span><textarea v-model="summary" rows="2"></textarea></label></div>
        <StructuredEditor v-model="documentJson" />
      </main>
      <aside class="editor-inspector"><div class="inspector-content"><p class="eyebrow">NEWS METADATA</p><h2>发布信息</h2><label class="field"><span>Slug</span><input v-model="slug" /></label><label class="field"><span>分类</span><input v-model="category" /></label><label class="field"><span>作者显示</span><input v-model="authorDisplayName" /></label><label class="field"><span>发布日期</span><input v-model="publishedAt" type="datetime-local" /></label><label class="field"><span>封面媒体 ID</span><input v-model="coverMediaId" /></label><label class="toggle-row"><span><strong>Featured</strong><small>由公开站排序使用</small></span><input v-model="featured" type="checkbox" /></label><label class="toggle-row"><span><strong>开发占位</strong><small>强制 noindex</small></span><input v-model="isPlaceholder" type="checkbox" /></label><p class="inspector-section-label">SEO</p><label class="field"><span>SEO Title</span><input v-model="seoTitle" /></label><label class="field"><span>Description</span><textarea v-model="seoDescription" rows="3"></textarea></label><label class="toggle-row"><span><strong>允许索引</strong><small>占位内容始终忽略</small></span><input v-model="indexable" type="checkbox" :disabled="isPlaceholder" /></label><p class="inspector-section-label">IMMUTABLE REVISIONS</p><label class="field"><span>回滚目标</span><select v-model.number="selectedRevision"><option v-for="item in revisions" :key="item.content.currentRevision" :value="item.content.currentRevision">Revision {{ item.content.currentRevision }} · {{ item.content.status }}</option></select></label><p v-if="!revisions.length" class="muted-copy">尚无可回滚的发布快照。</p><div v-else class="revision-diff"><strong>与当前 working draft 的 JSON diff（{{ revisionDiff.length }}）</strong><dl><div v-for="difference in revisionDiff.slice(0, 30)" :key="difference.path"><dt><code>{{ difference.path }}</code></dt><dd><del>{{ displayJson(difference.before) }}</del><ins>{{ displayJson(difference.after) }}</ins></dd></div></dl><p v-if="!revisionDiff.length" class="muted-copy">所选 revision 与当前表单一致。</p></div></div></aside>
    </div>
  </div>
</template>
