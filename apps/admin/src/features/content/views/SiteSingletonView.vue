<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { FilePenLine, Plus } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import type { CmsPublishedContent, ContentDraftV2 } from '@airtek/contracts'
import ContentEditorShell from '@/features/content/components/ContentEditorShell.vue'
import DataStatePanel from '@/shared/components/DataStatePanel.vue'
import PageHeader from '@/shared/components/PageHeader.vue'
import { contentApi } from '@/features/content/services/contentApi'
import { draftFromTemplate } from '@/features/content/services/contentDraftDefaults'
import { useContentEditorStore } from '@/features/content/stores/contentEditor'
import { useUiStore } from '@/shared/stores/ui'

type Section = 'general-information' | 'navigation' | 'footer'
type SingletonKind = 'generalInformation' | 'navigation' | 'footer'
type SingletonConfiguration = {
  kind: SingletonKind
  templateKey: 'generalInformation' | 'navigation' | 'footer'
  title: string
  description: string
}

const CONFIGURATION: Record<Section, SingletonConfiguration> = {
  'general-information': {
    kind: 'generalInformation',
    templateKey: 'generalInformation',
    title: '站点基本信息',
    description: '品牌、联系方式、社交链接、默认 SEO 与产品分类等站点级配置。',
  },
  navigation: {
    kind: 'navigation',
    templateKey: 'navigation',
    title: '站点导航',
    description: '主导航树、层级与链接目标；不产生独立公开路由。',
  },
  footer: {
    kind: 'footer',
    templateKey: 'footer',
    title: '站点页脚',
    description: '页脚栏目、法律链接与页脚文案。',
  },
}

const props = defineProps<{ section: Section }>()
const store = useContentEditorStore()
const ui = useUiStore()
const loading = ref(true)
const loadError = ref('')
const forbidden = ref(false)
const mutating = ref(false)
const published = ref<CmsPublishedContent | null>(null)

const configuration = computed(() => CONFIGURATION[props.section])

async function load(): Promise<void> {
  loading.value = true
  loadError.value = ''
  forbidden.value = false
  try {
    const state = await contentApi.getSiteSingleton(configuration.value.kind)
    published.value = state.published
    if (state.ownDraft) await store.load(state.ownDraft.draftId)
    else {
      store.reset()
      store.loadState = 'empty'
    }
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) forbidden.value = true
    else loadError.value = error instanceof Error ? error.message : `${configuration.value.title} 加载失败。`
  } finally {
    loading.value = false
  }
}

async function initialize(): Promise<void> {
  mutating.value = true
  try {
    const templates = await contentApi.listTemplates()
    const template = templates.find((entry) => entry.key === configuration.value.templateKey)
    if (!template) throw new Error('模板注册表中缺少该模板。')
    const draft: ContentDraftV2 = draftFromTemplate({
      template,
      title: configuration.value.title,
      isPlaceholder: true,
    })
    await store.create(draft)
    ui.toast('配置文档已创建', `${configuration.value.title} 已初始化为占位内容（noindex）。`)
  } catch (error) {
    ui.toast('初始化失败', error instanceof Error ? error.message : '请重试。', 'danger')
  } finally {
    mutating.value = false
  }
}

async function createEditingDraft(): Promise<void> {
  if (!published.value || mutating.value) return
  mutating.value = true
  try {
    const result = await contentApi.copyPublished(published.value.contentId)
    await store.load(result.draft.draftId)
    ui.toast('编辑草稿已创建', '已从当前发布版本复制；公开内容尚未改变。')
  } catch (error) {
    ui.toast('创建草稿失败', error instanceof Error ? error.message : '请重试。', 'danger')
  } finally {
    mutating.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="SITE" :title="configuration.title" :description="configuration.description" />
    <div v-if="loading" class="panel"><DataStatePanel state="loading" /></div>
    <DataStatePanel v-else-if="forbidden" state="forbidden" title="没有访问该站点配置的权限" @retry="load" />
    <DataStatePanel v-else-if="loadError" state="error" :title="loadError" @retry="load" />
    <section v-else-if="store.loadState === 'empty' && published" class="panel site-empty">
      <FilePenLine :size="22" />
      <h2>当前已有发布版本</h2>
      <p>访问本页不会创建或修改数据库记录。需要调整时，请显式创建自己的编辑草稿。</p>
      <p>发布版本 {{ published.publicationVersion }} · {{ new Date(published.updatedAt).toLocaleString('zh-CN') }}</p>
      <button class="button button--primary" type="button" :disabled="mutating" @click="createEditingDraft">
        {{ mutating ? '正在创建…' : '创建编辑草稿' }}
      </button>
    </section>
    <section v-else-if="store.loadState === 'empty'" class="panel site-empty">
      <Plus :size="22" />
      <h2>尚未初始化{{ configuration.title }}</h2>
      <p>该配置是每个 locale 一条的单例文档；初始化后才能进入编辑器。</p>
      <button class="button button--primary" type="button" :disabled="mutating" @click="initialize">
        {{ mutating ? '正在创建…' : `初始化${configuration.title}` }}
      </button>
    </section>
    <ContentEditorShell v-else @reload="load" />
  </div>
</template>

<style scoped>
@layer components {
.site-empty { display: flex; flex-direction: column; align-items: flex-start; gap: .6rem; }
.site-empty h2 { margin: 0; font-size: 1rem; }
.site-empty p { margin: 0; color: var(--text-secondary); font-size: .88rem; }
}
</style>
