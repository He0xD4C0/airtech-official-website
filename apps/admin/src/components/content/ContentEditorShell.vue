<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, CloudDownload, FileJson2, History, Save, Send, Settings2, ShieldAlert } from 'lucide-vue-next'
import type {
  AssetVersionReference,
  ContentBlock,
  ContentBlockKind,
  ContentDiffV2,
  ContentDraftV2,
  ContentRelationReference,
  ContentTypeFields,
  RelationTargetReference,
  SeoInputV2,
} from '@airtek/contracts'
import BlockInspector from '@/components/content/BlockInspector.vue'
import CompositionEditor from '@/components/content/CompositionEditor.vue'
import ConflictDialog from '@/components/content/ConflictDialog.vue'
import ContentMediaBlockDialog from '@/components/content/ContentMediaBlockDialog.vue'
import ContentOutline from '@/components/content/ContentOutline.vue'
import ContentPublishDialog from '@/components/content/ContentPublishDialog.vue'
import RevisionDiff from '@/components/content/RevisionDiff.vue'
import RevisionTimeline from '@/components/content/RevisionTimeline.vue'
import SeoInspector from '@/components/content/SeoInspector.vue'
import StructuredBodyEditor from '@/components/content/StructuredBodyEditor.vue'
import TypeFieldsPanel from '@/components/content/TypeFieldsPanel.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { blockKindLabels, contentKindLabels, contentStatusLabels } from '@/components/content/labels'
import { defaultBlock, newDraftId } from '@/services/contentDraftDefaults'
import { apiErrorMessage } from '@/services/cursorPagination'
import { useAuthStore } from '@/stores/auth'
import { useContentEditorStore } from '@/stores/contentEditor'
import { useUiStore } from '@/stores/ui'

const MANUAL_SNAPSHOT_REASON = 'Create a manual snapshot from the Admin CMS editor'
const DEFAULT_PUBLISH_REASON = 'Publish the current Admin CMS draft'

const emit = defineEmits<{ reload: [] }>()

const router = useRouter()
const auth = useAuthStore()
const ui = useUiStore()
const store = useContentEditorStore()

const activePanel = ref<'block' | 'seo' | 'revisions'>('seo')
const activeSection = ref('editor-basics')
const selectedBlockId = ref<string | null>(null)
const publishOpen = ref(false)
const publishReason = ref(DEFAULT_PUBLISH_REASON)
const publishing = ref(false)
const mediaDialogKind = ref<ContentBlockKind | null>(null)
const diff = ref<ContentDiffV2 | null>(null)
const diffLoading = ref(false)
const diffError = ref('')

const draft = computed(() => store.draft)
const template = computed(() => store.template)
const blocks = computed<ContentBlock[]>(() => draft.value?.composition.blocks ?? [])
const bodyPolicy = computed(() => template.value?.bodyPolicy ?? 'optional')
const selectedBlock = computed(() => blocks.value.find((block) => block.id === selectedBlockId.value) ?? null)
const statusTone = computed(() => {
  const status = store.record?.status
  if (status === 'published') return 'success' as const
  if (status === 'archived') return 'warning' as const
  return 'neutral' as const
})
const canPublish = computed(() => auth.hasPermission('content.publish'))
const isSingleton = computed(() => template.value?.routable === false)

const sections = computed(() => [
  { id: 'editor-basics', label: '基本信息' },
  { id: 'editor-composition', label: '页面组成', level: 1 as const },
  ...(bodyPolicy.value === 'forbidden' ? [] : [{ id: 'editor-body', label: '正文', level: 1 as const }]),
  { id: 'editor-type-fields', label: '类型字段', level: 1 as const },
])

const publishIssues = computed(() => {
  const current = draft.value
  const definition = template.value
  if (!current || !definition) return []
  const issues: string[] = []
  if (definition.routable && !(current.slug ?? '').trim()) issues.push('发布前必须填写 slug。')
  if (!current.title.trim()) issues.push('发布前必须填写标题。')
  const kinds = new Set(current.composition.blocks.map((block) => block.type))
  for (const required of definition.requiredBlocks) {
    if (!kinds.has(required)) issues.push(`缺少必需区块：${blockKindLabels[required] ?? required}`)
  }
  const hasBody = Boolean(current.body?.content.length)
  if (definition.bodyPolicy === 'required' && !hasBody) issues.push('该模板要求非空正文。')
  if (hasBody && !kinds.has('body')) issues.push('正文非空时需要在组成中加入正文区块。')
  return issues
})

watch(selectedBlockId, (value) => {
  if (value) activePanel.value = 'block'
})

function patchDraft(mutator: Parameters<typeof store.patch>[0]): void {
  store.patch(mutator)
}

function updateBlocks(next: ContentBlock[]): void {
  patchDraft((value) => {
    value.composition.blocks = next
  })
}

function addBlock(kind: ContentBlockKind): void {
  if (kind === 'media' || kind === 'downloadAsset') {
    mediaDialogKind.value = kind
    return
  }
  const block = defaultBlock(kind, draft.value?.title ?? null)
  if (!block) return
  updateBlocks([...blocks.value, block])
  selectedBlockId.value = block.id
}

function confirmMediaBlock(asset: AssetVersionReference): void {
  const kind = mediaDialogKind.value
  if (!kind) return
  const block: ContentBlock = kind === 'media'
    ? { type: 'media', id: newDraftId(), media: { asset, altText: null, decorative: false }, caption: null, layout: 'inline' }
    : { type: 'downloadAsset', id: newDraftId(), asset, label: '', description: null }
  updateBlocks([...blocks.value, block])
  selectedBlockId.value = block.id
  mediaDialogKind.value = null
}

function updateBlock(block: ContentBlock): void {
  updateBlocks(blocks.value.map((entry) => (entry.id === block.id ? block : entry)))
}

function removeBlock(id: string): void {
  updateBlocks(blocks.value.filter((entry) => entry.id !== id))
  if (selectedBlockId.value === id) selectedBlockId.value = null
}

function addRelation(target: RelationTargetReference, slot: string): void {
  const relation: ContentRelationReference = { id: newDraftId(), slot, target }
  patchDraft((value) => {
    value.relations.push(relation)
  })
}

function goToSection(id: string): void {
  activeSection.value = id
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function togglePlaceholder(checked: boolean): void {
  patchDraft((value) => {
    value.isPlaceholder = checked
    if (checked) value.seo.indexable = false
  })
}

function updateTitle(event: Event): void {
  const title = (event.target as HTMLInputElement).value
  patchDraft((value) => {
    value.title = title
  })
}

function updateSummary(event: Event): void {
  const summary = (event.target as HTMLTextAreaElement).value
  patchDraft((value) => {
    value.summary = summary || null
  })
}

function updateBody(value: ContentDraftV2['body']): void {
  patchDraft((draft) => {
    draft.body = value
  })
}

function updateTypeFields(value: ContentTypeFields): void {
  patchDraft((draft) => {
    draft.typeFields = value
  })
}

function updateSeo(value: SeoInputV2): void {
  patchDraft((draft) => {
    draft.seo = value
  })
}

function updateSlug(value: string | null): void {
  patchDraft((draft) => {
    draft.slug = value
  })
}

async function saveNow(): Promise<void> {
  const saved = await store.save()
  if (saved) ui.toast('草稿已保存', '正在写入 unified CMS working draft。')
  else if (store.saveState === 'conflict') ui.toast('检测到并发编辑', '自动保存已停止，请处理版本冲突。', 'warning')
  else if (store.saveError) ui.toast('保存失败', store.saveError, 'danger')
}

async function manualSnapshot(): Promise<void> {
  try {
    const result = await store.snapshot('manual', MANUAL_SNAPSHOT_REASON)
    if (result) ui.toast('快照已创建', '当前草稿已写入不可变 revision。')
  } catch (error) {
    ui.toast('快照失败', apiErrorMessage(error, '请检查权限与会话。'), 'danger')
  }
}

async function publish(): Promise<void> {
  publishing.value = true
  try {
    const result = await store.snapshot('publish', publishReason.value.trim() || DEFAULT_PUBLISH_REASON)
    if (result) {
      publishOpen.value = false
      ui.toast('内容已发布', `Published revision ${result.publishedRevision}.`)
    } else if (store.saveState === 'conflict') {
      ui.toast('保存已停止', '发布前发现版本冲突，请先处理差异。', 'warning')
    }
  } catch (error) {
    ui.toast('发布失败', apiErrorMessage(error, '请检查字段、权限与并发版本。'), 'danger')
  } finally {
    publishing.value = false
  }
}

async function compareRevisions(baseRevision: number, targetRevision?: number): Promise<void> {
  if (!store.record) return
  diffLoading.value = true
  diffError.value = ''
  try {
    diff.value = await store.compare(baseRevision, targetRevision)
  } catch (error) {
    diffError.value = apiErrorMessage(error, '无法读取修订差异。')
  } finally {
    diffLoading.value = false
  }
}

async function restoreRevision(revision: number, reason: string): Promise<void> {
  try {
    const restored = await store.restoreRevision(revision, reason)
    if (restored) ui.toast('已恢复修订', `Draft 已基于 revision ${revision} 重建。`)
  } catch (error) {
    ui.toast('恢复失败', apiErrorMessage(error, '请检查权限与会话。'), 'danger')
  }
}

function downloadConflictJson(): void {
  const blob = new Blob([store.conflictJson()], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `content-${store.record?.id ?? 'draft'}-local-draft.json`
  anchor.click()
  URL.revokeObjectURL(url)
}

async function reloadConflict(): Promise<void> {
  try {
    await store.reloadServerVersion()
    ui.toast('已重新载入', '本地未保存更改已丢弃，编辑器与服务器版本一致。')
  } catch (error) {
    ui.toast('重新载入失败', apiErrorMessage(error, '请重试。'), 'danger')
  }
}

async function loadRevisions(): Promise<void> {
  try {
    await store.loadRevisions()
  } catch (error) {
    ui.toast('修订列表加载失败', apiErrorMessage(error, '请重试。'), 'danger')
  }
}
</script>

<template>
  <div v-if="store.loadState !== 'ready'" class="page-stack">
    <DataStatePanel
      :state="store.loadState === 'forbidden' ? 'forbidden' : store.loadState === 'empty' ? 'empty' : store.loadState === 'loading' ? 'loading' : 'error'"
      :title="store.loadState === 'empty' ? '内容草稿不存在' : ''"
      :description="store.loadState === 'error' ? store.loadError : ''"
      @retry="emit('reload')"
    />
  </div>

  <div v-else-if="draft && template" class="content-editor">
    <header class="content-editor__topbar">
      <div class="content-editor__identity">
        <button class="icon-button" type="button" aria-label="返回内容中心" @click="router.push('/content')">
          <ArrowLeft :size="18" />
        </button>
        <div>
          <span>{{ contentKindLabels[template.contentKind] ?? template.contentKind }} · {{ template.key }}</span>
          <strong>{{ draft.title || '未命名内容' }}</strong>
        </div>
        <StatusBadge
          :label="contentStatusLabels[store.record?.status ?? 'draft'] ?? '草稿'"
          :tone="statusTone"
        />
      </div>
      <div class="content-editor__actions">
        <span class="save-state" aria-live="polite">{{ store.saveLabel }}</span>
        <button class="button button--quiet" type="button" :disabled="store.saveState === 'conflict'" @click="saveNow">
          <Save :size="15" />保存
        </button>
        <button class="button button--secondary" type="button" @click="manualSnapshot">
          <FileJson2 :size="15" />建立快照
        </button>
        <button v-if="canPublish" class="button button--primary" type="button" @click="publishOpen = true">
          <Send :size="15" />发布
        </button>
      </div>
    </header>

    <div v-if="store.conflict && store.conflictDismissed" class="conflict-banner" role="alert">
      <ShieldAlert :size="17" />
      <span>检测到并发编辑，自动保存已停止。</span>
      <button class="button button--quiet" type="button" @click="store.reopenConflict()">查看差异</button>
    </div>

    <div class="content-editor__layout">
      <ContentOutline :sections="sections" :active-id="activeSection" @select="goToSection" />

      <div class="content-editor__workspace">
        <section id="editor-basics" class="panel editor-section">
          <h2>基本信息</h2>
          <label class="field"><span>标题</span>
            <input
              :value="draft.title"
              maxlength="200"
              placeholder="公开页面标题"
              @input="updateTitle"
            />
          </label>
          <label class="field"><span>摘要</span>
            <textarea
              :value="draft.summary ?? ''"
              rows="2"
              maxlength="500"
              placeholder="用于列表、关联内容与 SEO 摘要"
              @input="updateSummary"
            />
          </label>
          <div class="editor-identity">
            <span>{{ isSingleton ? '站点配置文档（无公开路由）' : `模板 ${template.key} 不可切换` }}</span>
            <label class="toggle-row">
              <span><strong>占位内容</strong><small>占位内容强制 noindex，不进入 Sitemap</small></span>
              <input type="checkbox" :checked="draft.isPlaceholder" @change="togglePlaceholder(($event.target as HTMLInputElement).checked)" />
            </label>
          </div>
        </section>

        <section id="editor-composition" class="panel editor-section">
          <h2>页面组成</h2>
          <CompositionEditor
            :model-value="blocks"
            :template="template"
            :selected-block-id="selectedBlockId"
            @update:model-value="updateBlocks"
            @select="selectedBlockId = $event"
            @add-block="addBlock"
            @remove-block="removeBlock"
          />
        </section>

        <section v-if="bodyPolicy !== 'forbidden'" id="editor-body" class="panel editor-section">
          <h2>正文</h2>
          <StructuredBodyEditor
            :model-value="draft.body"
            :policy="bodyPolicy"
            @update:model-value="updateBody"
          />
        </section>

        <section id="editor-type-fields" class="panel editor-section">
          <h2>类型字段</h2>
          <TypeFieldsPanel
            :model-value="draft.typeFields"
            :kind="draft.kind"
            @update:model-value="updateTypeFields"
          />
        </section>
      </div>

      <aside class="content-editor__inspector" aria-label="编辑器检查器">
        <div class="inspector-tabs" role="tablist">
          <button type="button" role="tab" :aria-selected="activePanel === 'block'" :class="{ 'is-active': activePanel === 'block' }" @click="activePanel = 'block'">
            <Settings2 :size="15" />区块
          </button>
          <button type="button" role="tab" :aria-selected="activePanel === 'seo'" :class="{ 'is-active': activePanel === 'seo' }" @click="activePanel = 'seo'">
            <CloudDownload :size="15" />SEO
          </button>
          <button type="button" role="tab" :aria-selected="activePanel === 'revisions'" :class="{ 'is-active': activePanel === 'revisions' }" @click="activePanel = 'revisions'; loadRevisions()">
            <History :size="15" />修订
          </button>
        </div>

        <div v-if="activePanel === 'block'" class="inspector-panel">
          <BlockInspector
            v-if="selectedBlock"
            :model-value="selectedBlock"
            :relations="draft.relations"
            @update:model-value="updateBlock"
            @remove="removeBlock(selectedBlock.id)"
            @add-relation="addRelation"
          />
          <p v-else class="inspector-empty">在“页面组成”中选择一个区块以编辑其字段。</p>
        </div>

        <div v-else-if="activePanel === 'seo'" class="inspector-panel">
          <SeoInspector
            :model-value="draft.seo"
            :slug="draft.slug ?? null"
            :locale="draft.locale"
            :kind="draft.kind"
            :template="template"
            :is-placeholder="draft.isPlaceholder"
            @update:model-value="updateSeo"
            @update:slug="updateSlug"
          />
        </div>

        <div v-else class="inspector-panel">
          <RevisionTimeline
            :revisions="store.revisions"
            :current-draft-version="draft.draftVersion"
            :published-revision="store.record?.publishedRevision ?? null"
            @refresh="loadRevisions"
            @compare="compareRevisions"
            @restore="restoreRevision"
          />
          <RevisionDiff
            v-if="diff || diffLoading || diffError"
            :changes="diff?.changes ?? []"
            :loading="diffLoading"
            :error="diffError"
            empty-message="所选修订与目标之间没有差异。"
          />
        </div>
      </aside>
    </div>

    <ConflictDialog
      :open="Boolean(store.conflict && !store.conflictDismissed)"
      :local-draft="store.conflict?.localDraft ?? null"
      :server-draft="store.conflict?.serverRecord?.draft ?? null"
      :changes="store.conflict?.changes ?? []"
      @download="downloadConflictJson"
      @reload="reloadConflict"
      @close="store.dismissConflict()"
    />

    <ContentPublishDialog
      :open="publishOpen"
      :is-placeholder="draft.isPlaceholder"
      :issues="publishIssues"
      :reason="publishReason"
      :publishing="publishing"
      @update:reason="publishReason = $event"
      @confirm="publish"
      @close="publishOpen = false"
    />

    <ContentMediaBlockDialog
      :kind="mediaDialogKind"
      @confirm="confirmMediaBlock"
      @close="mediaDialogKind = null"
    />
  </div>
</template>

<style scoped>
.content-editor { display: flex; flex-direction: column; gap: 1rem; }
.content-editor__topbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: .75rem; }
.content-editor__identity { display: flex; align-items: center; gap: .65rem; }
.content-editor__identity span { display: block; font-size: .72rem; letter-spacing: .12em; text-transform: uppercase; color: var(--color-text-muted, #6b7280); }
.content-editor__identity strong { font-size: 1.05rem; }
.content-editor__actions { display: flex; align-items: center; gap: .5rem; }
.save-state { font-size: .8rem; color: var(--color-text-muted, #6b7280); }
.conflict-banner { display: flex; align-items: center; gap: .6rem; padding: .6rem .8rem; border: 1px solid #f0b429; border-radius: .5rem; background: #fff8e6; }
.content-editor__layout { display: grid; grid-template-columns: minmax(160px, 200px) minmax(0, 1fr) minmax(280px, 360px); gap: 1rem; align-items: start; }
.content-editor__workspace { display: flex; flex-direction: column; gap: 1rem; min-width: 0; }
.editor-section { display: flex; flex-direction: column; gap: .75rem; }
.editor-section h2 { font-size: 1rem; margin: 0; }
.editor-identity { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: .5rem; font-size: .8rem; color: var(--color-text-muted, #6b7280); }
.content-editor__inspector { display: flex; flex-direction: column; gap: .75rem; }
.inspector-tabs { display: flex; gap: .25rem; }
.inspector-tabs button { display: inline-flex; align-items: center; gap: .35rem; padding: .45rem .6rem; border: 1px solid transparent; border-radius: .4rem; background: transparent; cursor: pointer; }
.inspector-tabs button.is-active { border-color: var(--color-border, #d1d5db); background: var(--color-surface, #fff); font-weight: 600; }
.inspector-panel { display: flex; flex-direction: column; gap: .75rem; }
.inspector-empty { font-size: .85rem; color: var(--color-text-muted, #6b7280); }
@media (max-width: 1280px) { .content-editor__layout { grid-template-columns: 1fr; } }
</style>
