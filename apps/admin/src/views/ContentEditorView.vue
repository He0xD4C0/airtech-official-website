<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, CalendarClock, Check, ChevronDown, Eye, FileJson2, Globe2, Link2, Save, Send, Settings2 } from 'lucide-vue-next'
import DataStatePanel from '@/components/DataStatePanel.vue'
import StructuredEditor from '@/components/StructuredEditor.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { adminApi, type BackendContentEntry, type ContentDraftPayload } from '@/services/adminApi'
import {
  sanitizeContentDocumentAttrs,
  type ArticleAuthorType,
  type DownloadAccessStatus,
  type DownloadScanStatus,
} from '@/services/contentDocumentAttrs'
import { apiErrorMessage, apiProblemStatus } from '@/services/cursorPagination'
import { canPersistEditorRecord, editorFailureState, type EditorRecordState } from '@/services/editorRecordState'
import { useAuthStore } from '@/stores/auth'
import { useUiStore } from '@/stores/ui'

interface EditorDocument { type: 'doc'; attrs?: Record<string, unknown>; content?: Array<Record<string, unknown>>; schemaVersion?: number }

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const isNew = route.params.id === 'new'
const activePanel = ref<'document' | 'seo' | 'relations'>('document')
const saving = ref(false)
const previewing = ref(false)
const savedAt = ref('尚未保存')
const title = ref('')
const slug = ref(route.params.id === 'new' ? '' : String(route.params.id ?? ''))
const summary = ref('')
const seoTitle = ref('')
const seoDescription = ref('')
const indexable = ref(false)
const isPlaceholder = ref(true)
const contentKind = ref<ContentDraftPayload['kind']>('article')
const articleAuthor = ref('')
const articleAuthorType = ref<ArticleAuthorType | ''>('')
const articlePublishedAt = ref('')
const articleCategory = ref('')
const downloadVersion = ref('')
const downloadApplicableModels = ref('')
const downloadResourceType = ref('')
const downloadFileDescription = ref('')
const downloadUrl = ref('')
const downloadScanStatus = ref<DownloadScanStatus | ''>('')
const downloadAccessStatus = ref<DownloadAccessStatus | ''>('')
const entryId = ref<string>()
const revision = ref<number>()
const hydrating = ref(true)
const state = ref<EditorRecordState>(isNew ? 'ready' : 'loading')
const loadError = ref('')
const documentJson = ref<EditorDocument>({
  type: 'doc' as const,
  schemaVersion: 1,
  content: [],
})
let autosaveTimer: number | undefined

const canonicalPreview = computed(() => {
  const normalizedSlug = slug.value.replace(/^\/+|\/+$/g, '')
  if (contentKind.value === 'home' || normalizedSlug === 'home') return '/en'
  return normalizedSlug ? `/en/${normalizedSlug}` : '/en'
})

const contentKindLabel = computed(() => ({
  home: 'Home',
  article: 'Article',
  solution: 'Solution',
  technology: 'Technology',
  faq: 'FAQ',
  caseStudy: 'Case Study',
  download: 'Download',
  news: 'News',
  company: 'Company',
  legal: 'Legal',
  navigation: 'Navigation',
  footer: 'Footer',
})[contentKind.value] ?? contentKind.value)

function modelsFromInput(value: string): string[] {
  return value.split(/\r?\n|,/u).map((item) => item.trim()).filter(Boolean)
}

function currentDocumentAttrs(): Record<string, unknown> {
  if (contentKind.value === 'article') {
    return sanitizeContentDocumentAttrs('article', {
      author: articleAuthor.value,
      authorType: articleAuthorType.value,
      publishedAt: articlePublishedAt.value,
      category: articleCategory.value,
    })
  }
  if (contentKind.value === 'download') {
    return sanitizeContentDocumentAttrs('download', {
      version: downloadVersion.value,
      applicableModels: modelsFromInput(downloadApplicableModels.value),
      resourceType: downloadResourceType.value,
      fileDescription: downloadFileDescription.value,
      downloadUrl: downloadUrl.value,
      fileStatus: { scan: downloadScanStatus.value, access: downloadAccessStatus.value },
    })
  }
  return {}
}

function hydrateDocumentAttrs(kind: string, value: unknown): Record<string, unknown> {
  const attrs = sanitizeContentDocumentAttrs(kind, value)
  articleAuthor.value = typeof attrs.author === 'string' ? attrs.author : ''
  articleAuthorType.value = attrs.authorType === 'Person' || attrs.authorType === 'Organization' ? attrs.authorType : ''
  articlePublishedAt.value = typeof attrs.publishedAt === 'string' ? attrs.publishedAt : ''
  articleCategory.value = typeof attrs.category === 'string' ? attrs.category : ''
  downloadVersion.value = typeof attrs.version === 'string' ? attrs.version : ''
  downloadApplicableModels.value = Array.isArray(attrs.applicableModels) ? attrs.applicableModels.join('\n') : ''
  downloadResourceType.value = typeof attrs.resourceType === 'string' ? attrs.resourceType : ''
  downloadFileDescription.value = typeof attrs.fileDescription === 'string' ? attrs.fileDescription : ''
  downloadUrl.value = typeof attrs.downloadUrl === 'string' ? attrs.downloadUrl : ''
  const fileStatus = typeof attrs.fileStatus === 'object' && attrs.fileStatus !== null
    ? attrs.fileStatus as Record<string, unknown>
    : {}
  downloadScanStatus.value = typeof fileStatus.scan === 'string' ? fileStatus.scan as DownloadScanStatus : ''
  downloadAccessStatus.value = typeof fileStatus.access === 'string' ? fileStatus.access as DownloadAccessStatus : ''
  return attrs
}

function documentRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
}

function payload(): ContentDraftPayload {
  const attrs = currentDocumentAttrs()
  return {
    kind: contentKind.value,
    slug: slug.value.trim(),
    locale: 'en',
    title: title.value.trim(),
    summary: summary.value.trim() || null,
    body: {
      schemaVersion: 1,
      doc: {
        type: 'doc',
        ...(Object.keys(attrs).length ? { attrs } : {}),
        content: documentJson.value.content ?? [],
      },
    },
    seo: {
      title: seoTitle.value.trim() || null,
      description: seoDescription.value.trim() || null,
      canonicalPath: canonicalPreview.value,
      indexable: indexable.value && !isPlaceholder.value,
    },
    isPlaceholder: isPlaceholder.value,
  }
}

function applyEntry(entry: BackendContentEntry, hydrateDocument = true): void {
  entryId.value = entry.id
  revision.value = entry.currentRevision
  if (hydrateDocument) {
    title.value = entry.title
    slug.value = entry.slug
    summary.value = entry.summary ?? ''
    seoTitle.value = entry.seo.title ?? ''
    seoDescription.value = entry.seo.description ?? ''
    contentKind.value = entry.kind
    indexable.value = entry.seo.indexable
    isPlaceholder.value = entry.isPlaceholder
    const doc = documentRecord(entry.body.doc)
    const attrs = hydrateDocumentAttrs(entry.kind, doc.attrs)
    documentJson.value = {
      type: 'doc',
      ...(Object.keys(attrs).length ? { attrs } : {}),
      content: Array.isArray(doc.content) ? doc.content as Array<Record<string, unknown>> : [],
      schemaVersion: 1,
    }
  }
}

async function save(silent = false): Promise<boolean> {
  if (!canPersistEditorRecord(state.value, isNew, entryId.value)) return false
  saving.value = true
  try {
    const result = await adminApi.saveContent(payload(), entryId.value, revision.value)
    applyEntry(result.entry, false)
    savedAt.value = new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit' }).format(new Date())
    if (!silent) ui.toast('草稿已保存', '已写入 working draft，并保留当前公开 revision。')
    return true
  } catch (error) {
    ui.toast('草稿保存失败', error instanceof Error ? error.message : '请检查字段和并发版本。', 'danger')
    return false
  } finally {
    saving.value = false
  }
}

async function preview(): Promise<void> {
  previewing.value = true
  try {
    const saved = await save(true)
    if (!saved || !entryId.value || !revision.value) return
    const result = await adminApi.snapshotContent(entryId.value, revision.value)
    applyEntry(result.entry, false)
    ui.toast('手动快照已建立', '当前草稿已写入不可变 revision。')
  } catch (error) {
    ui.toast('快照创建失败', error instanceof Error ? error.message : '请检查权限、会话与并发版本。', 'danger')
  } finally {
    previewing.value = false
  }
}

async function publish(): Promise<void> {
  if (!auth.hasPermission('content.publish')) {
    ui.toast('没有发布权限', '当前账号可以编辑草稿，但不能发布内容。', 'warning')
    return
  }
  if (isPlaceholder.value) {
    ui.toast('占位内容将保持 noindex', '可以发布占位页用于受控展示，但它不会进入 Sitemap 或结构化数据。', 'warning')
  }
  const saved = await save(true)
  if (!saved) return
  if (!entryId.value || !revision.value) return
  try {
    const result = await adminApi.publishContent(entryId.value, revision.value)
    applyEntry(result.entry, false)
    ui.toast('内容已发布', `Published revision ${result.entry.publishedRevision}.`)
  } catch (error) {
    ui.toast('发布失败', error instanceof Error ? error.message : '请检查权限和并发版本。', 'danger')
  }
}

watch([
  documentJson,
  title,
  slug,
  summary,
  seoTitle,
  seoDescription,
  indexable,
  isPlaceholder,
  contentKind,
  articleAuthor,
  articleAuthorType,
  articlePublishedAt,
  articleCategory,
  downloadVersion,
  downloadApplicableModels,
  downloadResourceType,
  downloadFileDescription,
  downloadUrl,
  downloadScanStatus,
  downloadAccessStatus,
], () => {
  if (hydrating.value || state.value !== 'ready') return
  window.clearTimeout(autosaveTimer)
  autosaveTimer = window.setTimeout(() => {
    void save(true)
  }, 900)
}, { deep: true })

async function loadEntry(): Promise<void> {
  if (isNew) {
    state.value = 'ready'
    await nextTick()
    hydrating.value = false
    return
  }

  hydrating.value = true
  state.value = 'loading'
  loadError.value = ''
  entryId.value = undefined
  revision.value = undefined
  try {
    const entry = await adminApi.findContent(String(route.params.id))
    if (!entry) {
      state.value = 'empty'
      return
    }
    applyEntry(entry)
    state.value = 'ready'
  } catch (error) {
    state.value = editorFailureState(apiProblemStatus(error))
    loadError.value = apiErrorMessage(error, '请检查 API 会话后重试。')
  } finally {
    await nextTick()
    hydrating.value = false
  }
}

onMounted(async () => {
  await loadEntry()
})

onBeforeUnmount(() => window.clearTimeout(autosaveTimer))
</script>

<template>
  <div v-if="state !== 'ready'" class="page-stack">
    <DataStatePanel
      :state="state"
      :title="state === 'empty' ? '内容记录不存在' : ''"
      :description="state === 'empty' ? '该编辑地址没有对应的 working draft；请返回内容中心创建记录。' : state === 'error' ? loadError : ''"
      @retry="loadEntry"
    />
  </div>
  <div v-else class="editor-page">
    <header class="editor-topbar">
      <div class="editor-topbar__left">
        <button type="button" class="icon-button" aria-label="返回内容中心" @click="router.push('/content')"><ArrowLeft :size="19" /></button>
        <div><span>{{ contentKindLabel }} · English</span><strong>{{ title || '未命名内容' }}</strong></div>
        <StatusBadge label="草稿" tone="neutral" />
      </div>
      <div class="editor-topbar__right">
        <span class="save-state"><Check :size="14" />{{ saving ? '正在保存…' : savedAt }}</span>
        <button class="button button--quiet" type="button" @click="save(false)"><Save :size="16" />保存</button>
        <button class="button button--secondary" type="button" :disabled="previewing || saving" @click="preview"><Eye :size="16" />{{ previewing ? '正在建立…' : '建立快照' }}</button>
        <button v-if="auth.hasPermission('content.publish')" class="button button--primary" type="button" @click="publish"><Send :size="16" />发布<ChevronDown :size="14" /></button>
      </div>
    </header>

    <div class="editor-layout">
      <main class="editor-workspace">
        <div class="editor-title-fields">
          <label><span>标题</span><input v-model="title" placeholder="输入公开页面标题" /></label>
          <label><span>摘要</span><textarea v-model="summary" rows="2" placeholder="用于列表、关联内容和 SEO 摘要。不要填写未经验证的产品声明。"></textarea></label>
        </div>
        <StructuredEditor v-model="documentJson" />
      </main>

      <aside class="editor-inspector">
        <div class="inspector-tabs">
          <button type="button" :class="{ 'is-active': activePanel === 'document' }" aria-label="文档设置" @click="activePanel = 'document'"><Settings2 :size="17" /></button>
          <button type="button" :class="{ 'is-active': activePanel === 'seo' }" aria-label="SEO 设置" @click="activePanel = 'seo'"><Globe2 :size="17" /></button>
          <button type="button" :class="{ 'is-active': activePanel === 'relations' }" aria-label="内容关系" @click="activePanel = 'relations'"><Link2 :size="17" /></button>
        </div>

        <div v-if="activePanel === 'document'" class="inspector-content">
          <p class="eyebrow">DOCUMENT</p><h2>文档设置</h2>
          <label class="field"><span>内容类型</span><select v-model="contentKind"><option value="home">Home</option><option value="article">Article</option><option value="solution">Solution</option><option value="technology">Technology</option><option value="faq">FAQ</option><option value="caseStudy">Case Study</option><option value="download">Download</option><option value="company">Company</option><option value="legal">Legal</option></select></label>
          <label class="field"><span>语言</span><select><option>English (en)</option></select></label>
          <template v-if="contentKind === 'article'">
            <p class="inspector-section-label">文章发布信息</p>
            <label class="field"><span>作者</span><input v-model="articleAuthor" maxlength="160" placeholder="仅填写已确认的作者或团队名称" /></label>
            <label class="field"><span>作者类型</span><select v-model="articleAuthorType"><option value="">未指定</option><option value="Person">Person</option><option value="Organization">Organization</option></select></label>
            <label class="field"><span>发布日期</span><input v-model="articlePublishedAt" maxlength="40" placeholder="YYYY-MM-DD 或 RFC3339" /></label>
            <label class="field"><span>分类</span><input v-model="articleCategory" maxlength="120" placeholder="已发布内容分类" /></label>
          </template>
          <template v-else-if="contentKind === 'download'">
            <p class="inspector-section-label">受控下载信息</p>
            <label class="field"><span>版本</span><input v-model="downloadVersion" maxlength="80" placeholder="已确认的文件版本" /></label>
            <label class="field"><span>适用型号</span><textarea v-model="downloadApplicableModels" rows="3" placeholder="每行一个稳定型号；也可用逗号分隔"></textarea></label>
            <label class="field"><span>资源类型</span><input v-model="downloadResourceType" maxlength="80" placeholder="例如已批准的资源分类" /></label>
            <label class="field"><span>文件说明</span><textarea v-model="downloadFileDescription" rows="3" maxlength="500" placeholder="准确描述当前受控文件"></textarea></label>
            <label class="field"><span>下载 URL</span><input v-model="downloadUrl" maxlength="2048" placeholder="/同源路径 或 https://" /></label>
            <label class="field"><span>扫描状态</span><select v-model="downloadScanStatus"><option value="">未指定</option><option value="clean">clean</option><option value="pending">pending</option><option value="scanning">scanning</option><option value="quarantined">quarantined</option><option value="blocked">blocked</option><option value="failed">failed</option><option value="missing">missing</option></select></label>
            <label class="field"><span>访问状态</span><select v-model="downloadAccessStatus"><option value="">未指定</option><option value="public">public</option><option value="private">private</option><option value="restricted">restricted</option></select></label>
            <p class="inspector-copy">公开下载按钮只会在页面可索引、文件为 clean/public 且 URL 通过安全校验时出现。</p>
          </template>
          <p v-else-if="contentKind === 'faq'" class="inspector-copy">FAQ 问题使用正文中的 H2–H4 问句，后续结构化区块作为答案；FAQ 不接受自由根属性。</p>
          <div class="inspector-card"><CalendarClock :size="17" /><div><strong>计划发布</strong><p>未设置日期，将在授权用户确认后立即发布。</p></div><button type="button">设置</button></div>
          <div class="inspector-card"><FileJson2 :size="17" /><div><strong>Schema v1</strong><p>正文以结构化 Tiptap JSON 保存。</p></div></div>
        </div>

        <div v-else-if="activePanel === 'seo'" class="inspector-content">
          <p class="eyebrow">SEARCH</p><h2>SEO 与路由</h2>
          <label class="field"><span>Slug</span><div class="field__prefix"><span>/en/</span><input v-model="slug" /></div></label>
          <label class="field"><span>SEO title</span><input v-model="seoTitle" placeholder="50–60 characters" /></label>
          <label class="field"><span>Meta description</span><textarea v-model="seoDescription" rows="4" placeholder="Describe the visible page content accurately."></textarea></label>
          <label class="toggle-row"><span><strong>占位内容</strong><small>占位内容强制 noindex</small></span><input v-model="isPlaceholder" type="checkbox" /></label>
          <label class="toggle-row"><span><strong>允许索引</strong><small>仅非占位内容可开启</small></span><input v-model="indexable" type="checkbox" :disabled="isPlaceholder" /></label>
          <div class="canonical-preview"><span>Canonical preview</span><code>{{ canonicalPreview }}</code></div>
        </div>

        <div v-else class="inspector-content">
          <p class="eyebrow">RELATIONS</p><h2>内容关系</h2>
          <p class="inspector-copy">显式关联产品、Solution、Technology、FAQ、Case 和 Download；不根据关键词自动生成声明。</p>
          <button class="relation-picker" type="button"><span>＋</span><div><strong>添加关联产品</strong><p>通过稳定产品 ID 选择</p></div></button>
          <button class="relation-picker" type="button"><span>＋</span><div><strong>添加关联内容</strong><p>建立可审核的双向内链</p></div></button>
          <div class="empty-mini">尚未配置关系</div>
        </div>
      </aside>
    </div>
  </div>
</template>
