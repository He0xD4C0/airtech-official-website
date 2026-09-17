<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, Eye, Redo2, Save, Send, Settings2, Undo2 } from 'lucide-vue-next'
import type {
  AssetVersionReference,
  ContentBlock,
  ContentBlockKind,
  ContentDraftV2,
  ContentRelationReference,
  ContentTypeFields,
  RelationTargetReference,
  SeoInputV2,
} from '@airtek/contracts'
import BlockInspector from '@/components/content/BlockInspector.vue'
import CompositionEditor from '@/components/content/CompositionEditor.vue'
import ContentMediaBlockDialog from '@/components/content/ContentMediaBlockDialog.vue'
import ContentOutline from '@/components/content/ContentOutline.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import SeoInspector from '@/components/content/SeoInspector.vue'
import StructuredBodyEditor from '@/components/content/StructuredBodyEditor.vue'
import TypeFieldsPanel from '@/components/content/TypeFieldsPanel.vue'
import { defaultBlock, newDraftId } from '@/services/contentDraftDefaults'
import { useContentEditorStore } from '@/stores/contentEditor'
import { useUiStore } from '@/stores/ui'

const emit = defineEmits<{ reload: [] }>()
const router = useRouter()
const store = useContentEditorStore()
const ui = useUiStore()
const activePanel = ref<'block' | 'seo'>('seo')
const activeSection = ref('editor-basics')
const selectedBlockId = ref<string | null>(null)
const mediaDialogKind = ref<ContentBlockKind | null>(null)
const previewOpen = ref(false)

const draft = computed(() => store.draft)
const template = computed(() => store.template)
const blocks = computed<ContentBlock[]>(() => draft.value?.composition.blocks ?? [])
const bodyPolicy = computed(() => template.value?.bodyPolicy ?? 'optional')
const selectedBlock = computed(() => blocks.value.find((block) => block.id === selectedBlockId.value) ?? null)
const editable = computed(() => store.record?.state === 'editing')
const sections = computed(() => [
  { id: 'editor-basics', label: '基本信息' },
  { id: 'editor-composition', label: '页面组成', level: 1 as const },
  ...(bodyPolicy.value === 'forbidden' ? [] : [{ id: 'editor-body', label: '正文', level: 1 as const }]),
  { id: 'editor-type-fields', label: '类型字段', level: 1 as const },
])

function patchDraft(
  key: string,
  mutator: (value: ContentDraftV2) => ContentDraftV2 | void,
): void {
  store.patch(mutator, { historyKey: key })
}

function updateBlocks(next: ContentBlock[]): void {
  patchDraft('composition', (value) => { value.composition.blocks = next })
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
  updateBlocks(blocks.value.map((entry) => entry.id === block.id ? block : entry))
}

function removeBlock(id: string): void {
  updateBlocks(blocks.value.filter((entry) => entry.id !== id))
  if (selectedBlockId.value === id) selectedBlockId.value = null
}

function addRelation(target: RelationTargetReference, slot: string): void {
  const relation: ContentRelationReference = { id: newDraftId(), slot, target }
  patchDraft('relations', (value) => { value.relations.push(relation) })
}

function goToSection(id: string): void {
  activeSection.value = id
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function selectInspector(panel: 'block' | 'seo'): void {
  activePanel.value = panel
  document.getElementById(`inspector-tab-${panel}`)?.focus()
}

function onInspectorKeydown(event: KeyboardEvent): void {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  if (event.key === 'Home') selectInspector('block')
  else if (event.key === 'End') selectInspector('seo')
  else selectInspector(activePanel.value === 'block' ? 'seo' : 'block')
}

async function save(): Promise<void> {
  if (await store.save()) ui.toast('草稿已保存', '当前私人草稿已写入数据库。')
  else ui.toast('保存失败', store.saveError || '请重试。', 'danger')
}

async function submit(): Promise<void> {
  try {
    const result = await store.submit()
    if (!result) return
    if (result.status === 'published' && result.publication) {
      ui.toast('已发布', `当前发布版本 ${result.publication.publicationVersion}。`)
      await router.push(`/content/published/${result.publication.contentId}`)
    } else {
      ui.toast('已提交审核', '草稿已冻结，等待 content.publish 审核人处理。')
    }
  } catch (error) {
    ui.toast('提交失败', error instanceof Error ? error.message : '请重试。', 'danger')
  }
}
</script>

<template>
  <div v-if="store.loadState !== 'ready'" class="page-stack">
    <DataStatePanel
      :state="store.loadState === 'forbidden' ? 'forbidden' : store.loadState === 'empty' ? 'empty' : store.loadState === 'loading' ? 'loading' : 'error'"
      :title="store.loadState === 'empty' ? '私人草稿不存在' : store.loadError"
      @retry="emit('reload')"
    />
  </div>

  <div v-else-if="draft && template" class="content-editor">
    <header class="content-editor__topbar">
      <div class="content-editor__identity">
        <button class="icon-button" type="button" aria-label="返回私人草稿" @click="router.push('/content/drafts')"><ArrowLeft :size="18" /></button>
        <div><span>{{ template.contentKind }} · {{ template.key }}</span><strong>{{ draft.title || '未命名内容' }}</strong></div>
      </div>
      <div class="content-editor__actions">
        <span class="save-state">{{ store.saveLabel }}</span>
        <button class="icon-button" type="button" :disabled="!store.canUndo || !editable" aria-label="撤销" @click="store.undo"><Undo2 :size="16" /></button>
        <button class="icon-button" type="button" :disabled="!store.canRedo || !editable" aria-label="重做" @click="store.redo"><Redo2 :size="16" /></button>
        <button class="button button--quiet" type="button" aria-controls="draft-local-preview" :aria-expanded="previewOpen" @click="previewOpen = !previewOpen"><Eye :size="15" />本地预览</button>
        <button class="button button--quiet" type="button" :disabled="!store.isDirty || !editable || store.saveState === 'saving'" @click="save"><Save :size="15" />保存</button>
        <button class="button button--primary" type="button" :disabled="store.isDirty || !editable" @click="submit"><Send :size="15" />提交审核</button>
      </div>
    </header>

    <p v-if="!editable" class="pending-banner">该草稿处于待审核状态，当前只读；所有者可在草稿列表撤回。</p>
    <p v-if="store.saveError" class="save-error">{{ store.saveError }} <button class="button button--quiet" @click="store.reloadServerVersion">重新载入</button></p>

    <section v-if="previewOpen" id="draft-local-preview" class="panel local-preview" aria-label="编辑器内存预览">
      <small>LOCAL MEMORY PREVIEW · 不创建 URL 或数据库记录</small>
      <h1>{{ draft.title }}</h1><p>{{ draft.summary }}</p>
      <ol><li v-for="block in draft.composition.blocks" :key="block.id">{{ block.type }}</li></ol>
    </section>

    <div class="content-editor__layout" :inert="!editable">
      <ContentOutline :sections="sections" :active-id="activeSection" @select="goToSection" />
      <div class="content-editor__workspace">
        <section id="editor-basics" class="panel editor-section">
          <h2>基本信息</h2>
          <label class="field"><span>标题</span><input :value="draft.title" maxlength="200" @input="patchDraft('title', value => { value.title = ($event.target as HTMLInputElement).value })" /></label>
          <label class="field"><span>摘要</span><textarea :value="draft.summary ?? ''" rows="2" maxlength="500" @input="patchDraft('summary', value => { value.summary = ($event.target as HTMLTextAreaElement).value || null })" /></label>
          <label class="toggle-row"><span><strong>占位内容</strong><small>占位内容强制 noindex</small></span><input type="checkbox" :checked="draft.isPlaceholder" @change="patchDraft('placeholder', value => { value.isPlaceholder = ($event.target as HTMLInputElement).checked; if (value.isPlaceholder) value.seo.indexable = false })" /></label>
        </section>
        <section id="editor-composition" class="panel editor-section"><h2>页面组成</h2><CompositionEditor :model-value="blocks" :template="template" :selected-block-id="selectedBlockId" @update:model-value="updateBlocks" @select="selectedBlockId = $event" @add-block="addBlock" @remove-block="removeBlock" /></section>
        <section v-if="bodyPolicy !== 'forbidden'" id="editor-body" class="panel editor-section"><h2>正文</h2><StructuredBodyEditor :model-value="draft.body" :policy="bodyPolicy" @update:model-value="patchDraft('body', value => { value.body = $event })" /></section>
        <section id="editor-type-fields" class="panel editor-section"><h2>类型字段</h2><TypeFieldsPanel :model-value="draft.typeFields" :kind="draft.kind" @update:model-value="patchDraft('typeFields', value => { value.typeFields = $event as ContentTypeFields })" /></section>
      </div>
      <aside class="content-editor__inspector" aria-label="内容检查器">
        <div class="inspector-tabs" role="tablist" aria-label="检查器面板" @keydown="onInspectorKeydown">
          <button id="inspector-tab-block" type="button" role="tab" aria-controls="inspector-panel-block" :aria-selected="activePanel === 'block'" :tabindex="activePanel === 'block' ? 0 : -1" :class="{ 'is-active': activePanel === 'block' }" @click="selectInspector('block')"><Settings2 :size="15" />区块</button>
          <button id="inspector-tab-seo" type="button" role="tab" aria-controls="inspector-panel-seo" :aria-selected="activePanel === 'seo'" :tabindex="activePanel === 'seo' ? 0 : -1" :class="{ 'is-active': activePanel === 'seo' }" @click="selectInspector('seo')">SEO</button>
        </div>
        <section v-if="activePanel === 'block'" id="inspector-panel-block" role="tabpanel" aria-labelledby="inspector-tab-block">
          <BlockInspector v-if="selectedBlock" :model-value="selectedBlock" :relations="draft.relations" @update:model-value="updateBlock" @remove="removeBlock(selectedBlock.id)" @add-relation="addRelation" />
          <div v-else class="panel inspector-empty"><strong>尚未选择区块</strong><p>从页面组成中选择一个区块后，可在这里编辑其属性。</p></div>
        </section>
        <section v-else id="inspector-panel-seo" role="tabpanel" aria-labelledby="inspector-tab-seo">
          <SeoInspector :model-value="draft.seo" :slug="draft.slug ?? null" :locale="draft.locale" :template="template" :is-placeholder="draft.isPlaceholder" @update:model-value="patchDraft('seo', value => { value.seo = $event as SeoInputV2 })" @update:slug="patchDraft('slug', value => { value.slug = $event })" />
        </section>
      </aside>
    </div>
    <ContentMediaBlockDialog :kind="mediaDialogKind" @confirm="confirmMediaBlock" @close="mediaDialogKind = null" />
  </div>
</template>

<style scoped>
.content-editor { display:flex; flex-direction:column; gap:1rem }.content-editor__topbar,.content-editor__actions,.content-editor__identity { display:flex; align-items:center; gap:.55rem }.content-editor__topbar { justify-content:space-between; flex-wrap:wrap }.content-editor__identity span { display:block; color:var(--admin-muted); font-size:.75rem }.content-editor__layout { display:grid; grid-template-columns:180px minmax(0,1fr) 320px; gap:1rem; align-items:start }.content-editor__workspace,.content-editor__inspector,.editor-section { display:flex; flex-direction:column; gap:.75rem }.save-state { color:var(--admin-muted); font-size:.8rem }.pending-banner,.save-error { padding:.65rem; border-radius:.45rem; background:#fff8e6 }.local-preview { border:2px solid var(--airtek-blue); }.local-preview small { color:var(--admin-muted) }.inspector-tabs { display:flex; gap:.25rem }.inspector-tabs button { padding:.5rem; border:1px solid transparent; background:white }.inspector-tabs .is-active { border-color:var(--admin-line) }.inspector-empty p { margin:0; color:var(--admin-muted) }[inert] { opacity:.72 }@media(max-width:1280px){.content-editor__layout{grid-template-columns:1fr}}
</style>
