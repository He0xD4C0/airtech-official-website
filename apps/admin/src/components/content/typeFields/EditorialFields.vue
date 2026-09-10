<script setup lang="ts">
import { computed } from 'vue'
import type { ContentTypeFields, MediaUseReference } from '@airtek/contracts'
import MediaAssetField from '../fields/MediaAssetField.vue'

const props = defineProps<{ modelValue: ContentTypeFields }>()
const emit = defineEmits<{ 'update:modelValue': [value: ContentTypeFields] }>()

const editorial = computed(() => (
  props.modelValue.type === 'article' || props.modelValue.type === 'news' ? props.modelValue : null
))
const keyed = computed(() => (
  props.modelValue.type === 'solution' || props.modelValue.type === 'technology' ? props.modelValue : null
))
const caseStudy = computed(() => (props.modelValue.type === 'caseStudy' ? props.modelValue : null))
const download = computed(() => (props.modelValue.type === 'download' ? props.modelValue : null))
const legal = computed(() => (props.modelValue.type === 'legal' ? props.modelValue : null))

function patch(values: Partial<{ [key: string]: unknown }>): void {
  emit('update:modelValue', { ...props.modelValue, ...values } as ContentTypeFields)
}

function toDateTimeLocal(iso: string | null | undefined): string {
  if (!iso) return ''
  const parsed = new Date(iso)
  if (Number.isNaN(parsed.getTime())) return ''
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())}T${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`
}

function fromDateTimeLocal(value: string): string | null {
  if (!value) return null
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? null : parsed.toISOString()
}
</script>

<template>
  <div class="type-fields">
    <p v-if="modelValue.type === 'home' || modelValue.type === 'page' || modelValue.type === 'company'" class="inspector-copy">
      该内容类型没有额外的结构化字段，页面结构由「页面组成」中的受控区块决定。
    </p>

    <template v-else-if="keyed">
      <label class="field">
        <span>稳定键<small>用于选择器与关联引用，只允许小写字母、数字和连字符</small></span>
        <input
          :value="keyed.key ?? ''"
          maxlength="120"
          placeholder="例如 ie3-motors"
          pattern="[a-z0-9-]*"
          @input="patch({ key: ($event.target as HTMLInputElement).value.trim() || null })"
        />
      </label>
    </template>

    <template v-else-if="editorial">
      <label class="field">
        <span>分类<small>用于列表分组与筛选</small></span>
        <input
          :value="editorial.category ?? ''"
          maxlength="120"
          placeholder="例如 Company news"
          @input="patch({ category: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
      <label class="field">
        <span>作者显示名</span>
        <input
          :value="editorial.authorDisplayName ?? ''"
          maxlength="120"
          @input="patch({ authorDisplayName: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
      <label class="field">
        <span>发布时间<small>留空表示暂无发布时间</small></span>
        <input
          type="datetime-local"
          :value="toDateTimeLocal(editorial.publicationAt)"
          @input="patch({ publicationAt: fromDateTimeLocal(($event.target as HTMLInputElement).value) })"
        />
      </label>
      <div class="field">
        <span>封面媒体</span>
        <MediaAssetField
          :model-value="editorial.cover ?? null"
          mode="media"
          label="封面媒体"
          @update:model-value="patch({ cover: $event as MediaUseReference | null })"
        />
      </div>
      <label class="toggle-row">
        <span><strong>精选内容</strong><small>精选内容可出现在首页或列表页的推荐位</small></span>
        <input
          type="checkbox"
          :checked="editorial.featured"
          @change="patch({ featured: ($event.target as HTMLInputElement).checked })"
        />
      </label>
    </template>

    <template v-else-if="caseStudy">
      <label class="field">
        <span>行业</span>
        <input
          :value="caseStudy.industry ?? ''"
          maxlength="120"
          placeholder="例如 Data center"
          @input="patch({ industry: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
      <label class="field">
        <span>地点</span>
        <input
          :value="caseStudy.location ?? ''"
          maxlength="120"
          @input="patch({ location: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
    </template>

    <template v-else-if="download">
      <label class="field">
        <span>资源类型<small>例如 Datasheet、Manual、Certificate</small></span>
        <input
          :value="download.resourceType ?? ''"
          maxlength="120"
          @input="patch({ resourceType: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
      <label class="field">
        <span>版本标签</span>
        <input
          :value="download.versionLabel ?? ''"
          maxlength="120"
          @input="patch({ versionLabel: ($event.target as HTMLInputElement).value || null })"
        />
      </label>
      <label class="field">
        <span>版本说明</span>
        <textarea
          :value="download.versionNotes ?? ''"
          rows="2"
          maxlength="500"
          @input="patch({ versionNotes: ($event.target as HTMLTextAreaElement).value || null })"
        />
      </label>
    </template>

    <label v-else-if="legal" class="field">
      <span>生效日期<small>法律文本的生效时间</small></span>
      <input
        type="date"
        :value="legal.effectiveDate ?? ''"
        @input="patch({ effectiveDate: ($event.target as HTMLInputElement).value || null })"
      />
    </label>
  </div>
</template>

<style scoped>
.type-fields { display: flex; flex-direction: column; gap: 0.75rem; }
</style>
