<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, FilePlus2, Lock } from 'lucide-vue-next'
import { ApiError } from '@airtek/contracts'
import type { ContentTemplateDefinition } from '@airtek/contracts'
import DataStatePanel from '@/components/DataStatePanel.vue'
import PageHeader from '@/components/PageHeader.vue'
import { contentKindLabel } from '@/components/content/labels'
import { contentApi } from '@/services/contentApi'
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
const groups = computed(() => {
  const grouped = new Map<string, ContentTemplateDefinition[]>()
  for (const template of routableTemplates.value) {
    const list = grouped.get(template.contentKind) ?? []
    list.push(template)
    grouped.set(template.contentKind, list)
  }
  return [...grouped.entries()]
})
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
      slug: slug.value.trim() || null,
      isPlaceholder: isPlaceholder.value,
    })
    const id = await store.create(draft)
    ui.toast('草稿已创建', '类型与模板已锁定，可开始编辑。')
    await router.push(`/content/${id}/edit`)
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

onMounted(loadTemplates)
</script>

<template>
  <div class="page-stack">
    <PageHeader eyebrow="CONTENT / NEW" title="新建内容" description="选择受控模板后创建 working draft；类型与模板在创建后不可切换。">
      <template #actions>
        <RouterLink class="button button--quiet" to="/content"><ArrowLeft :size="15" />返回内容中心</RouterLink>
      </template>
    </PageHeader>

    <DataStatePanel
      v-if="pageState !== 'ready'"
      :state="pageState"
      :title="pageState === 'error' ? errorMessage : ''"
      @retry="loadTemplates"
    />

    <form v-else class="panel create-form" @submit.prevent="submit">
      <fieldset class="create-form__templates">
        <legend>受控模板</legend>
        <section v-for="[kind, list] in groups" :key="kind" class="create-form__group">
          <h2>{{ contentKindLabel(kind) }}</h2>
          <div class="create-form__cards">
            <label v-for="template in list" :key="template.key" class="template-card" :class="{ 'is-selected': selectedKey === template.key }">
              <input v-model="selectedKey" type="radio" name="content-template" :value="template.key" />
              <div>
                <strong>{{ template.key }}</strong>
                <span>{{ template.bodyPolicy === 'required' ? '需要正文' : template.bodyPolicy === 'forbidden' ? '无正文' : '正文可选' }} · {{ template.requiredBlocks.length ? `必需 ${template.requiredBlocks.join(', ')}` : '无必需区块' }}</span>
              </div>
            </label>
          </div>
        </section>
      </fieldset>

      <div class="create-form__fields">
        <label class="field"><span>标题</span>
          <input v-model="title" maxlength="200" required placeholder="公开页面标题" />
        </label>
        <label v-if="selectedTemplate && selectedTemplate.routable" class="field"><span>Slug（可选，发布前必须填写）</span>
          <input v-model="slug" maxlength="200" pattern="[a-z0-9]+(?:-[a-z0-9]+)*" placeholder="lowercase-with-hyphens" />
        </label>
        <label class="toggle-row">
          <span><strong>占位内容</strong><small>占位内容强制 noindex；由开发种子创建的内容在清除后归编辑部所有</small></span>
          <input v-model="isPlaceholder" type="checkbox" />
        </label>
        <p class="create-form__lock"><Lock :size="14" />创建后内容类型与模板不可切换</p>
        <p v-if="createError" class="create-form__error" role="alert">{{ createError }}</p>
      </div>

      <div class="create-form__actions">
        <button class="button button--quiet" type="button" @click="router.push('/content')">取消</button>
        <button class="button button--primary" type="submit" :disabled="!canSubmit"><FilePlus2 :size="15" />{{ creating ? '正在创建…' : '创建草稿' }}</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.create-form { display: flex; flex-direction: column; gap: 1rem; }
.create-form__templates { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: .8rem; }
.create-form__templates legend { font-weight: 600; margin-bottom: .4rem; }
.create-form__group h2 { font-size: .9rem; margin: 0 0 .35rem; }
.create-form__cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: .5rem; }
.template-card { display: flex; gap: .5rem; align-items: flex-start; border: 1px solid var(--color-border, #d1d5db); border-radius: .5rem; padding: .55rem .65rem; cursor: pointer; }
.template-card.is-selected { border-color: var(--color-primary, #2563eb); box-shadow: 0 0 0 1px var(--color-primary, #2563eb) inset; }
.template-card strong { display: block; }
.template-card span { font-size: .76rem; color: var(--color-text-muted, #6b7280); }
.create-form__fields { display: flex; flex-direction: column; gap: .65rem; }
.create-form__lock { display: inline-flex; align-items: center; gap: .35rem; font-size: .8rem; color: var(--color-text-muted, #6b7280); margin: 0; }
.create-form__error { color: #b91c1c; font-size: .85rem; margin: 0; }
.create-form__actions { display: flex; justify-content: flex-end; gap: .5rem; }
</style>
