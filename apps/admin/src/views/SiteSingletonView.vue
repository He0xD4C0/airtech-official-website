<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Plus } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import type { ContentDraftV2 } from '@airtek/contracts'
import ContentEditorShell from '@/components/content/ContentEditorShell.vue'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { contentApi } from '@/services/contentApi'
import { draftFromTemplate } from '@/services/contentDraftDefaults'
import { useContentEditorStore } from '@/stores/contentEditor'
import { useUiStore } from '@/stores/ui'

type Section = 'general-information' | 'navigation' | 'footer'

const props = defineProps<{ section: Section }>()

const store = useContentEditorStore()
const ui = useUiStore()
const loading = ref(true)
const loadError = ref('')
const forbiddden = ref(false)
const initializing = ref(false)

const configuration = computed(() => ({
  'general-information': {
    kind: 'generalInformation',
    templateKey: 'generalInformation',
    title: 'General Information',
    description: '品牌、联系方式、社交链接、默认 SEO 与产品分类等站点级配置。',
  },
  navigation: {
    kind: 'navigation',
    templateKey: 'navigation',
    title: 'Navigation',
    description: '主导航树、层级与链接目标；不产生公开路由。',
  },
  footer: {
    kind: 'footer',
    templateKey: 'footer',
    title: 'Footer',
    description: '页脚栏目、法律链接与页脚文案。',
  },
}[props.section]))

async function findRecordId(): Promise<string | null> {
  const page = await contentApi.listContent({ kinds: [configuration.value.kind], limit: 1 })
  return page.items[0]?.id ?? null
}

async function load(): Promise<void> {
  loading.value = true
  loadError.value = ''
  forbiddden.value = false
  try {
    const id = await findRecordId()
    if (id) {
      await store.load(id)
    } else {
      store.reset()
      store.loadState = 'empty'
    }
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) {
      forbiddden.value = true
    } else {
      loadError.value = error instanceof Error ? error.message : `${configuration.value.title} 加载失败。`
    }
  } finally {
    loading.value = false
  }
}

async function initialize(): Promise<void> {
  initializing.value = true
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
    initializing.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="SITE" :title="configuration.title" :description="configuration.description" />
    <div v-if="loading" class="panel"><DataStatePanel state="loading" /></div>
    <DataStatePanel
      v-else-if="forbiddden"
      state="forbidden"
      title="没有访问该站点配置的权限"
      @retry="load"
    />
    <DataStatePanel
      v-else-if="loadError"
      state="error"
      :title="loadError"
      @retry="load"
    />
    <div v-else-if="store.loadState === 'empty'" class="panel site-empty">
      <h2>尚未初始化 {{ configuration.title }}</h2>
      <p>该配置文档是单例（每 locale 一条），创建后可在此维护；创建时必须使用受控模板。</p>
      <button class="button button--primary" type="button" :disabled="initializing" @click="initialize">
        <Plus :size="15" />{{ initializing ? '正在创建…' : `初始化 ${configuration.title}` }}
      </button>
    </div>
    <ContentEditorShell v-else @reload="load" />
  </div>
</template>

<style scoped>
.site-empty { display: flex; flex-direction: column; gap: .6rem; align-items: flex-start; }
.site-empty h2 { margin: 0; font-size: 1rem; }
.site-empty p { margin: 0; font-size: .88rem; color: var(--color-text-muted, #6b7280); }
</style>
