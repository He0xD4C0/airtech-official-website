<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, ArrowRight, Check, FilePlus2, Lock } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import type { ContentTemplateDefinition } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { bodyPolicyLabel, contentKindLabel, contentTemplateLabels } from '@/components/content/labels'
import { contentApi } from '@/services/contentApi'
import { routePatternUsesSlug } from '@/services/canonicalPath'
import { draftFromTemplate } from '@/services/contentDraftDefaults'
import { useContentEditorStore } from '@/stores/contentEditor'
import { useUiStore } from '@/stores/ui'

type PageState = 'loading' | 'ready' | 'error' | 'forbidden'

const router = useRouter()
const store = useContentEditorStore()
const ui = useUiStore()

const templates = ref<ContentTemplateDefinition[]>([])
const pageState = ref<PageState>('loading')
const errorMessage = ref('')
const step = ref<1 | 2 | 3>(1)
const selectedKind = ref<ContentTemplateDefinition['contentKind'] | null>(null)
const selectedKey = ref<string | null>(null)
const title = ref('')
const slug = ref('')
const isPlaceholder = ref(true)
const creating = ref(false)
const createError = ref('')

const routableTemplates = computed(() => templates.value.filter((template) => template.routable))
const selectedTemplate = computed(() => (
  routableTemplates.value.find((template) => template.key === selectedKey.value) ?? null
))
const requiresSlug = computed(() => routePatternUsesSlug(selectedTemplate.value?.routePattern))
const kinds = computed(() => [...new Set(routableTemplates.value.map((template) => template.contentKind))])
const kindTemplates = computed(() => routableTemplates.value.filter((template) => template.contentKind === selectedKind.value))
const canSubmit = computed(() => Boolean(selectedTemplate.value) && title.value.trim().length > 0 && !creating.value)

async function loadTemplates(): Promise<void> {
  pageState.value = 'loading'
  try {
    templates.value = await contentApi.listTemplates()
    pageState.value = 'ready'
  } catch (error) {
    if (error instanceof ApiError && error.status === 403) {
      pageState.value = 'forbidden'
    } else {
      pageState.value = 'error'
      errorMessage.value = error instanceof Error ? error.message : '模板注册表加载失败。'
    }
  }
}

async function submit(): Promise<void> {
  const template = selectedTemplate.value
  if (!template || !canSubmit.value) return
  creating.value = true
  createError.value = ''
  try {
    const draft = draftFromTemplate({
      template,
      title: title.value.trim(),
      slug: requiresSlug.value ? slug.value.trim() || null : null,
      isPlaceholder: isPlaceholder.value,
    })
    const id = await store.create(draft)
    ui.toast('草稿已创建', '类型与模板已锁定，可开始编辑。')
    await router.push(`/content/drafts/${id}`)
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      createError.value = '同一 kind + slug + locale 已存在内容，请更换 slug。'
    } else {
      createError.value = error instanceof Error ? error.message : '创建草稿失败。'
    }
  } finally {
    creating.value = false
  }
}

function chooseKind(kind: ContentTemplateDefinition['contentKind']): void {
  selectedKind.value = kind
  selectedKey.value = null
  step.value = 2
}

function chooseTemplate(key: string): void {
  selectedKey.value = key
  step.value = 3
}

onMounted(loadTemplates)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / NEW" title="新建内容" description="选择受控模板后创建 working draft；类型与模板在创建后不可切换。">
      <template #actions>
        <RouterLink class="button button--quiet" to="/content/drafts"><ArrowLeft :size="15" />返回私人草稿</RouterLink>
      </template>
    </PageHeader>

    <DataStatePanel
      v-if="pageState !== 'ready'"
      :state="pageState"
      :title="pageState === 'error' ? errorMessage : ''"
      @retry="loadTemplates"
    />

    <form v-else class="panel create-form" @submit.prevent="submit">
      <ol class="create-steps" aria-label="新建内容步骤">
        <li v-for="item in [{ id: 1, label: '内容类别' }, { id: 2, label: '选择模板' }, { id: 3, label: '基本信息' }]" :key="item.id" :class="{ 'is-active': step === item.id, 'is-complete': step > item.id }"><span><Check v-if="step > item.id" :size="13" />{{ item.id }}</span>{{ item.label }}</li>
      </ol>

      <fieldset v-if="step === 1" class="create-form__templates">
        <legend>1. 选择内容类别</legend>
        <p class="create-form__hint">先按编辑用途选择类别，下一步只显示该类别可用的受控模板。</p>
        <div class="create-form__cards">
          <button v-for="kind in kinds" :key="kind" class="template-card" type="button" @click="chooseKind(kind)"><div><strong>{{ contentKindLabel(kind) }}</strong><span>{{ routableTemplates.filter((template) => template.contentKind === kind).length }} 个可用模板</span></div><ArrowRight :size="16" /></button>
        </div>
      </fieldset>

      <fieldset v-else-if="step === 2" class="create-form__templates">
        <legend>2. 选择{{ contentKindLabel(selectedKind || '') }}模板</legend>
        <p class="create-form__hint">模板名称面向编辑人员；内部 key 只用于技术排障。</p>
        <div class="create-form__cards">
          <button v-for="template in kindTemplates" :key="template.key" class="template-card" type="button" @click="chooseTemplate(template.key)"><div><strong>{{ contentTemplateLabels[template.key] }}</strong><span>{{ bodyPolicyLabel(template.bodyPolicy) }} · {{ template.requiredBlocks.length ? `必需 ${template.requiredBlocks.join(', ')}` : '无必需区块' }}</span><code>{{ template.key }}</code></div><ArrowRight :size="16" /></button>
        </div>
        <button class="button button--quiet" type="button" @click="step = 1"><ArrowLeft :size="15" />返回选择类别</button>
      </fieldset>

      <div v-else class="create-form__fields">
        <div class="create-form__selection"><span>{{ contentKindLabel(selectedTemplate?.contentKind || '') }}</span><strong>{{ selectedTemplate ? contentTemplateLabels[selectedTemplate.key] : '' }}</strong><code>{{ selectedTemplate?.key }}</code></div>
        <label class="field"><span>标题</span>
          <input v-model="title" maxlength="200" required placeholder="公开页面标题" />
        </label>
        <label v-if="requiresSlug" class="field"><span>Slug（可选，发布前必须填写）</span>
          <input v-model="slug" maxlength="200" pattern="[a-z0-9]+(?:-[a-z0-9]+)*" placeholder="lowercase-with-hyphens" />
        </label>
        <label class="toggle-row">
          <span><strong>占位内容</strong><small>占位内容强制 noindex；由开发种子创建的内容在清除后归编辑部所有</small></span>
          <input v-model="isPlaceholder" type="checkbox" />
        </label>
        <p class="create-form__lock"><Lock :size="14" />创建后内容类型与模板不可切换</p>
        <p v-if="createError" class="create-form__error" role="alert">{{ createError }}</p>
      </div>

      <div v-if="step === 3" class="create-form__actions">
        <button class="button button--quiet" type="button" @click="step = 2"><ArrowLeft :size="15" />返回选择模板</button>
        <button class="button button--primary" type="submit" :disabled="!canSubmit"><FilePlus2 :size="15" />{{ creating ? '正在创建…' : '创建草稿' }}</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
@layer components {
.create-form { display: flex; flex-direction: column; gap: 1rem; }
.create-form__templates { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .8rem; }
.create-form__templates legend { font-weight: 600; margin-bottom: .4rem; }
.create-form__hint { margin: 0; color: var(--text-secondary, #556663); font-size: .78rem; }
.create-form__cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: .5rem; }
.template-card { display: flex; gap: .5rem; align-items: center; justify-content: space-between; border: 1px solid var(--border-default, #d1d5db); border-radius: .5rem; padding: .7rem; background: white; text-align: left; cursor: pointer; }
.template-card:hover, .template-card:focus-visible { border-color: var(--airtek-blue, #0c7497); }
.template-card strong { display: block; }
.template-card span { font-size: .76rem; color: var(--text-secondary, #556663); }
.template-card code { display: block; margin-top: .25rem; color: var(--text-secondary, #556663); font-size: 0.75rem; }
.create-steps { display: grid; grid-template-columns: repeat(3, 1fr); gap: .5rem; margin: 0; padding: 0; list-style: none; }
.create-steps li { display: flex; align-items: center; gap: .4rem; color: var(--text-secondary, #556663); font-size: .75rem; }
.create-steps span { display: grid; place-items: center; width: 1.55rem; height: 1.55rem; border-radius: 50%; background: #edf2f2; font-size: 0.75rem; }
.create-steps .is-active { color: var(--text-primary); font-weight: 700; }.create-steps .is-active span, .create-steps .is-complete span { background: var(--airtek-blue, #0c7497); color: white; }
.create-form__fields { display: flex; flex-direction: column; gap: .65rem; }
.create-form__selection { display: flex; align-items: center; gap: .5rem; padding: .6rem; border-radius: .5rem; background: #f5f8f8; }.create-form__selection span, .create-form__selection code { color: var(--text-secondary, #556663); font-size: 0.75rem; }.create-form__selection code { margin-left: auto; }
.create-form__lock { display: inline-flex; align-items: center; gap: .35rem; font-size: .8rem; color: var(--text-secondary, #556663); margin: 0; }
.create-form__error { color: #b91c1c; font-size: .85rem; margin: 0; }
.create-form__actions { display: flex; justify-content: flex-end; gap: .5rem; }
}
</style>
