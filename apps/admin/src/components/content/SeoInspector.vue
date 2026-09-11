<script setup lang="ts">
import { computed } from 'vue'
import { Globe2, Info, Link2, ShieldAlert } from 'lucide-vue-next'
import type { ContentTemplateDefinition, MediaUseReference, SeoInputV2 } from '@airtek/contracts'
import { canonicalPathForDraft, normalizeSlug, routePatternUsesSlug } from '@/services/canonicalPath'
import MediaAssetField from './fields/MediaAssetField.vue'

const props = defineProps<{
  modelValue: SeoInputV2
  slug: string | null
  locale: string
  template: ContentTemplateDefinition
  isPlaceholder: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: SeoInputV2]
  'update:slug': [value: string | null]
}>()

const TITLE_LIMIT = 60
const DESCRIPTION_LIMIT = 160

const routable = computed(() => props.template.routable)
const requiresSlug = computed(() => routePatternUsesSlug(props.template.routePattern))
const indexable = computed(() => props.modelValue.indexable && !props.isPlaceholder && routable.value)

const canonicalPath = computed(() => canonicalPathForDraft(
  { locale: props.locale, slug: props.slug },
  props.template,
))

const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u
const slugError = computed(() => {
  if (!routable.value || !requiresSlug.value) return ''
  const slug = normalizeSlug(props.slug)
  if (!slug) return '该模板可路由，slug 为发布必需字段。'
  if (!slugPattern.test(slug)) return 'slug 只允许小写字母、数字与连字符（例如 ie3-motors）。'
  return ''
})

const titleLength = computed(() => (props.modelValue.title ?? '').length)
const descriptionLength = computed(() => (props.modelValue.description ?? '').length)

function patch(values: Partial<SeoInputV2>): void {
  emit('update:modelValue', { ...props.modelValue, ...values })
}

function updateSlug(value: string): void {
  const normalized = normalizeSlug(value)
  emit('update:slug', normalized)
}

function toggleIndexable(enabled: boolean): void {
  if (props.isPlaceholder || !routable.value) return
  patch({ indexable: enabled })
}
</script>

<template>
  <div class="seo-inspector">
    <p v-if="isPlaceholder" class="seo-inspector__banner" role="status">
      <ShieldAlert :size="15" />
      <span>占位内容强制 noindex，且不会进入 sitemap；解除占位后才能开放索引。</span>
    </p>
    <p v-else-if="!routable" class="seo-inspector__banner" role="status">
      <Info :size="15" />
      <span>该模板是站点配置文档，没有公开路由，因此不需要 slug 或 canonical。</span>
    </p>

    <label v-if="routable && requiresSlug" class="field">
      <span>Slug<small>只允许小写字母、数字与连字符</small></span>
      <input
        :value="slug ?? ''"
        maxlength="160"
        placeholder="ie3-motors"
        :aria-invalid="Boolean(slugError)"
        aria-describedby="seo-slug-error seo-canonical"
        @input="updateSlug(($event.target as HTMLInputElement).value)"
      />
      <small v-if="slugError" id="seo-slug-error" class="seo-inspector__error">{{ slugError }}</small>
    </label>

    <div class="canonical-preview" id="seo-canonical">
      <span><Link2 :size="13" /> Canonical（由路由解析器生成，不接受手填）</span>
      <code>{{ canonicalPath ?? '该内容没有公开 URL（无 canonical）' }}</code>
    </div>

    <section class="seo-inspector__preview" aria-labelledby="seo-preview-title">
      <h3 id="seo-preview-title"><Globe2 :size="14" />搜索结果预览</h3>
      <div class="seo-inspector__serp">
        <p class="seo-inspector__serp-path">{{ canonicalPath ?? `/${locale}` }}</p>
        <p class="seo-inspector__serp-title">{{ modelValue.title || '（未填写 SEO 标题）' }}</p>
        <p class="seo-inspector__serp-description">{{ modelValue.description || '（未填写 SEO 描述）' }}</p>
        <p v-if="!indexable" class="seo-inspector__serp-note">noindex：该页面不会出现在搜索引擎结果中。</p>
      </div>
    </section>

    <label class="field">
      <span>SEO 标题<small :class="{ 'seo-inspector__over': titleLength > TITLE_LIMIT }">{{ titleLength }} / {{ TITLE_LIMIT }}</small></span>
      <input
        :value="modelValue.title ?? ''"
        maxlength="200"
        placeholder="保持 60 字符以内"
        @input="patch({ title: ($event.target as HTMLInputElement).value || null })"
      />
    </label>

    <label class="field">
      <span>SEO 描述<small :class="{ 'seo-inspector__over': descriptionLength > DESCRIPTION_LIMIT }">{{ descriptionLength }} / {{ DESCRIPTION_LIMIT }}</small></span>
      <textarea
        :value="modelValue.description ?? ''"
        rows="3"
        maxlength="500"
        placeholder="描述页面对用户的价值，避免堆砌关键词"
        @input="patch({ description: ($event.target as HTMLTextAreaElement).value || null })"
      />
    </label>

    <label class="toggle-row">
      <span>
        <strong>允许搜索引擎索引</strong>
        <small v-if="isPlaceholder">占位内容被强制 noindex，无法开启。</small>
        <small v-else-if="!routable">站点配置文档没有公开路由。</small>
        <small v-else>关闭后页面将输出 noindex,follow。</small>
      </span>
      <input
        type="checkbox"
        :checked="indexable"
        :disabled="isPlaceholder || !routable"
        @change="toggleIndexable(($event.target as HTMLInputElement).checked)"
      />
    </label>

    <div class="field">
      <span>社交分享图</span>
      <MediaAssetField
        :model-value="modelValue.socialImage ?? null"
        label="社交分享图"
        @update:model-value="patch({ socialImage: $event as MediaUseReference | null })"
      />
    </div>
  </div>
</template>

<style scoped>
.seo-inspector { display: flex; flex-direction: column; gap: 0.7rem; }
.seo-inspector__banner { display: flex; align-items: flex-start; gap: 0.4rem; padding: 0.55rem 0.6rem; margin: 0; border: 1px solid #f0dca8; border-radius: 8px; background: var(--admin-soft-amber); color: #8a6100; font-size: 0.6rem; line-height: 1.5; }
.seo-inspector__error { color: #b42318; }
.seo-inspector__over { color: #b42318; }
.seo-inspector__preview { display: flex; flex-direction: column; gap: 0.35rem; }
.seo-inspector__preview h3 { display: flex; align-items: center; gap: 0.3rem; margin: 0; color: var(--airtek-blue-dark); font-size: 0.62rem; letter-spacing: 0.05em; text-transform: uppercase; }
.seo-inspector__serp { padding: 0.6rem; border: 1px solid var(--admin-line); border-radius: 8px; background: white; }
.seo-inspector__serp-path { margin: 0 0 0.2rem; color: #3c7d3a; font-size: 0.56rem; overflow-wrap: anywhere; }
.seo-inspector__serp-title { margin: 0 0 0.15rem; color: #1a0dab; font-size: 0.72rem; }
.seo-inspector__serp-description { margin: 0; color: var(--admin-muted); font-size: 0.6rem; line-height: 1.5; }
.seo-inspector__serp-note { margin: 0.3rem 0 0; color: #8a6100; font-size: 0.55rem; }
</style>
