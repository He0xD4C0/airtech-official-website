<script setup lang="ts">
import { computed } from 'vue'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import type { AssetVersionReference, ContactChannelKind, ContentBlock, CtaBlock, EditorialAction, EvidenceItem, MediaUseReference, RelationCollectionBlock, ContentRelationReference, RelationTargetReference } from '@airtek/contracts'
import { newDraftId } from '@/features/content/services/contentDraftDefaults'
import EditorialActionField from '@/features/content/components/fields/EditorialActionField.vue'
import MediaAssetField from '@/features/content/components/fields/MediaAssetField.vue'
import RelationCollectionFields from '@/features/content/components/blockInspectors/RelationCollectionFields.vue'

const props = defineProps<{
  modelValue: ContentBlock
  relations: ContentRelationReference[]
}>()

const emit = defineEmits<{
  'update:modelValue': [value: ContentBlock]
  'add-relation': [target: RelationTargetReference, slot: string]
}>()

const hero = computed(() => (props.modelValue.type === 'hero' ? props.modelValue : null))
const media = computed(() => (props.modelValue.type === 'media' ? props.modelValue : null))
const featureGrid = computed(() => (props.modelValue.type === 'featureGrid' ? props.modelValue : null))
const evidence = computed(() => (props.modelValue.type === 'evidence' ? props.modelValue : null))
const cta = computed(() => (props.modelValue.type === 'cta' ? props.modelValue : null))
const contact = computed(() => (props.modelValue.type === 'contactBlock' ? props.modelValue : null))
const downloadAsset = computed(() => (props.modelValue.type === 'downloadAsset' ? props.modelValue : null))
const relationCollection = computed(() => (
  props.modelValue.type === 'relationCollection' ? props.modelValue as RelationCollectionBlock : null
))

const CHANNELS: { value: ContactChannelKind; label: string }[] = [
  { value: 'email', label: '邮箱' },
  { value: 'phone', label: '电话' },
  { value: 'address', label: '地址' },
  { value: 'social', label: '社交媒体' },
]

function patch(values: Record<string, unknown>): void {
  emit('update:modelValue', { ...props.modelValue, ...values } as ContentBlock)
}

function move<T>(list: T[], index: number, direction: 'up' | 'down'): T[] {
  const target = direction === 'up' ? index - 1 : index + 1
  if (target < 0 || target >= list.length) return list
  const next = [...list]
  const [entry] = next.splice(index, 1)
  next.splice(target, 0, entry)
  return next
}

function updateHeroAction(index: number, action: EditorialAction | null): void {
  if (!hero.value) return
  const actions = action
    ? hero.value.actions.map((entry, position) => (position === index ? action : entry))
    : hero.value.actions.filter((_, position) => position !== index)
  patch({ actions })
}

function addHeroAction(): void {
  if (!hero.value) return
  patch({ actions: [...hero.value.actions, { label: '', target: { targetType: 'route', path: '' } }] })
}

function moveHeroAction(index: number, direction: 'up' | 'down'): void {
  if (!hero.value) return
  patch({ actions: move(hero.value.actions, index, direction) })
}

function updateFeatureItem(index: number, values: Record<string, unknown>): void {
  if (!featureGrid.value) return
  patch({
    items: featureGrid.value.items.map((item, position) => (
      position === index ? { ...item, ...values } : item
    )),
  })
}

function updateFeatureIcon(index: number, icon: MediaUseReference | null): void {
  updateFeatureItem(index, { icon })
}

function updateEvidenceItem(index: number, values: Partial<EvidenceItem>): void {
  if (!evidence.value) return
  patch({
    items: evidence.value.items.map((item, position) => (
      position === index ? { ...item, ...values } : item
    )),
  })
}

function toggleChannel(channel: ContactChannelKind, enabled: boolean): void {
  if (!contact.value) return
  patch({
    channels: enabled
      ? [...contact.value.channels.filter((entry) => entry !== channel), channel]
      : contact.value.channels.filter((entry) => entry !== channel),
  })
}

function updateContactAction(action: EditorialAction | null): void {
  patch({ action })
}

function updateCtaAction(action: EditorialAction | null): void {
  if (!cta.value || !action) return
  const next: CtaBlock = { ...cta.value, action }
  emit('update:modelValue', next)
}
</script>

<template>
  <div class="block-fields">
    <template v-if="hero">
      <label class="field"><span>Eyebrow 眉标</span>
        <input :value="hero.eyebrow ?? ''" maxlength="120" @input="patch({ eyebrow: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field"><span>主标题</span>
        <input :value="hero.heading ?? ''" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field"><span>引导文案</span>
        <textarea :value="hero.lead ?? ''" rows="2" maxlength="500" @input="patch({ lead: ($event.target as HTMLTextAreaElement).value || null })" />
      </label>
      <label class="field"><span>版式</span>
        <select :value="hero.variant" @change="patch({ variant: ($event.target as HTMLSelectElement).value })">
          <option value="standard">standard</option>
          <option value="splitMedia">splitMedia</option>
          <option value="minimal">minimal</option>
        </select>
      </label>
      <div class="field"><span>背景媒体</span>
        <MediaAssetField :model-value="hero.media ?? null" label="Hero 媒体" @update:model-value="patch({ media: $event as MediaUseReference | null })" />
      </div>
      <div class="block-fields__list">
        <div class="block-fields__list-head">
          <strong>行动按钮（{{ hero.actions.length }}）</strong>
          <button class="button button--quiet" type="button" @click="addHeroAction"><Plus :size="14" />添加按钮</button>
        </div>
        <p v-if="!hero.actions.length" class="empty-mini">还没有行动按钮。</p>
        <div v-for="(action, index) in hero.actions" :key="`hero-action-${index}`" class="block-fields__item">
          <div class="block-fields__item-head">
            <strong>按钮 {{ index + 1 }}</strong>
            <div>
              <button class="icon-button" type="button" :disabled="index === 0" :aria-label="`上移按钮 ${index + 1}`" @click="moveHeroAction(index, 'up')"><ArrowUp :size="14" /></button>
              <button class="icon-button" type="button" :disabled="index === hero.actions.length - 1" :aria-label="`下移按钮 ${index + 1}`" @click="moveHeroAction(index, 'down')"><ArrowDown :size="14" /></button>
            </div>
          </div>
          <EditorialActionField :model-value="action" label="按钮" @update:model-value="updateHeroAction(index, $event)" />
        </div>
      </div>
    </template>

    <template v-else-if="media">
      <div class="field"><span>媒体资产</span>
        <MediaAssetField :model-value="media.media" label="媒体" @update:model-value="patch({ media: $event })" />
      </div>
      <label class="field"><span>图注</span>
        <input :value="media.caption ?? ''" maxlength="300" @input="patch({ caption: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field"><span>版式</span>
        <select :value="media.layout" @change="patch({ layout: ($event.target as HTMLSelectElement).value })">
          <option value="inline">inline</option>
          <option value="fullWidth">fullWidth</option>
          <option value="aside">aside</option>
        </select>
      </label>
    </template>

    <template v-else-if="modelValue.type === 'body'">
      <p class="inspector-copy">正文内容在「正文」编辑器中维护，这里只调整排版宽度。</p>
      <label class="field"><span>正文宽度</span>
        <select :value="modelValue.width" @change="patch({ width: ($event.target as HTMLSelectElement).value })">
          <option value="narrow">narrow</option>
          <option value="standard">standard</option>
          <option value="wide">wide</option>
        </select>
      </label>
    </template>

    <template v-else-if="featureGrid">
      <label class="field"><span>区块标题</span>
        <input :value="featureGrid.heading ?? ''" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <div class="block-fields__list">
        <div class="block-fields__list-head">
          <strong>特性条目（{{ featureGrid.items.length }}）</strong>
          <button class="button button--quiet" type="button" @click="patch({ items: [...featureGrid.items, { id: newDraftId(), title: '', description: null, icon: null }] })"><Plus :size="14" />添加条目</button>
        </div>
        <p v-if="!featureGrid.items.length" class="empty-mini">还没有特性条目。</p>
        <div v-for="(item, index) in featureGrid.items" :key="item.id" class="block-fields__item">
          <div class="block-fields__item-head">
            <strong>条目 {{ index + 1 }}</strong>
            <div>
              <button class="icon-button" type="button" :disabled="index === 0" :aria-label="`上移条目 ${index + 1}`" @click="patch({ items: move(featureGrid.items, index, 'up') })"><ArrowUp :size="14" /></button>
              <button class="icon-button" type="button" :disabled="index === featureGrid.items.length - 1" :aria-label="`下移条目 ${index + 1}`" @click="patch({ items: move(featureGrid.items, index, 'down') })"><ArrowDown :size="14" /></button>
              <button class="icon-button" type="button" :aria-label="`删除条目 ${index + 1}`" @click="patch({ items: featureGrid.items.filter((_, position) => position !== index) })"><Trash2 :size="14" /></button>
            </div>
          </div>
          <label class="field"><span>标题</span>
            <input :value="item.title" maxlength="200" :aria-label="`条目 ${index + 1} 标题`" @input="updateFeatureItem(index, { title: ($event.target as HTMLInputElement).value })" />
          </label>
          <label class="field"><span>描述</span>
            <textarea :value="item.description ?? ''" rows="2" maxlength="500" :aria-label="`条目 ${index + 1} 描述`" @input="updateFeatureItem(index, { description: ($event.target as HTMLTextAreaElement).value || null })" />
          </label>
          <div class="field"><span>图标</span>
            <MediaAssetField :model-value="item.icon ?? null" label="图标" @update:model-value="updateFeatureIcon(index, $event as MediaUseReference | null)" />
          </div>
        </div>
      </div>
    </template>

    <template v-else-if="evidence">
      <label class="field"><span>区块标题</span>
        <input :value="evidence.heading ?? ''" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <p class="inspector-copy">证据条目的陈述必须有出处；测试数据只能引用已核验事实。</p>
      <div class="block-fields__list">
        <div class="block-fields__list-head">
          <strong>证据条目（{{ evidence.items.length }}）</strong>
          <button class="button button--quiet" type="button" @click="patch({ items: [...evidence.items, { id: newDraftId(), label: '', statement: '', sourceNote: null }] })"><Plus :size="14" />添加证据</button>
        </div>
        <p v-if="!evidence.items.length" class="empty-mini">还没有证据条目。</p>
        <div v-for="(item, index) in evidence.items" :key="item.id" class="block-fields__item">
          <div class="block-fields__item-head">
            <strong>证据 {{ index + 1 }}</strong>
            <div>
              <button class="icon-button" type="button" :disabled="index === 0" :aria-label="`上移证据 ${index + 1}`" @click="patch({ items: move(evidence.items, index, 'up') })"><ArrowUp :size="14" /></button>
              <button class="icon-button" type="button" :disabled="index === evidence.items.length - 1" :aria-label="`下移证据 ${index + 1}`" @click="patch({ items: move(evidence.items, index, 'down') })"><ArrowDown :size="14" /></button>
              <button class="icon-button" type="button" :aria-label="`删除证据 ${index + 1}`" @click="patch({ items: evidence.items.filter((_, position) => position !== index) })"><Trash2 :size="14" /></button>
            </div>
          </div>
          <label class="field"><span>标签</span>
            <input :value="item.label" maxlength="120" :aria-label="`证据 ${index + 1} 标签`" @input="updateEvidenceItem(index, { label: ($event.target as HTMLInputElement).value })" />
          </label>
          <label class="field"><span>陈述</span>
            <textarea :value="item.statement" rows="2" maxlength="500" :aria-label="`证据 ${index + 1} 陈述`" @input="updateEvidenceItem(index, { statement: ($event.target as HTMLTextAreaElement).value })" />
          </label>
          <label class="field"><span>出处说明</span>
            <input :value="item.sourceNote ?? ''" maxlength="300" :aria-label="`证据 ${index + 1} 出处`" @input="updateEvidenceItem(index, { sourceNote: ($event.target as HTMLInputElement).value || null })" />
          </label>
        </div>
      </div>
    </template>

    <template v-else-if="cta">
      <label class="field"><span>Eyebrow 眉标</span>
        <input :value="cta.eyebrow ?? ''" maxlength="120" @input="patch({ eyebrow: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <label class="field"><span>标题</span>
        <input :value="cta.heading" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value })" />
      </label>
      <label class="field"><span>正文</span>
        <textarea :value="cta.body ?? ''" rows="2" maxlength="500" @input="patch({ body: ($event.target as HTMLTextAreaElement).value || null })" />
      </label>
      <label class="field"><span>版式</span>
        <select :value="cta.variant" @change="patch({ variant: ($event.target as HTMLSelectElement).value })">
          <option value="standard">standard</option>
          <option value="emphasized">emphasized</option>
        </select>
      </label>
      <EditorialActionField :model-value="cta.action" label="CTA 按钮" @update:model-value="updateCtaAction" />
    </template>

    <template v-else-if="relationCollection">
      <RelationCollectionFields
        :model-value="relationCollection"
        :relations="relations"
        @update:model-value="emit('update:modelValue', $event)"
        @add-relation="(target, slot) => emit('add-relation', target, slot)"
      />
    </template>

    <template v-else-if="downloadAsset">
      <div class="field"><span>下载资产</span>
        <MediaAssetField
          :model-value="downloadAsset.asset"
          mode="asset"
          label="下载文件"
          @update:model-value="patch({ asset: $event as AssetVersionReference })"
        />
      </div>
      <label class="field"><span>链接文案</span>
        <input :value="downloadAsset.label" maxlength="200" @input="patch({ label: ($event.target as HTMLInputElement).value })" />
      </label>
      <label class="field"><span>说明</span>
        <textarea :value="downloadAsset.description ?? ''" rows="2" maxlength="500" @input="patch({ description: ($event.target as HTMLTextAreaElement).value || null })" />
      </label>
    </template>

    <template v-else-if="contact">
      <label class="field"><span>区块标题</span>
        <input :value="contact.heading ?? ''" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <fieldset class="block-fields__channels">
        <legend>展示渠道</legend>
        <label v-for="channel in CHANNELS" :key="channel.value" class="toggle-row">
          <span><strong>{{ channel.label }}</strong></span>
          <input
            type="checkbox"
            :checked="contact.channels.includes(channel.value)"
            @change="toggleChannel(channel.value, ($event.target as HTMLInputElement).checked)"
          />
        </label>
      </fieldset>
      <EditorialActionField :model-value="contact.action ?? null" label="联系动作" @update:model-value="updateContactAction" />
    </template>

    <template v-else-if="modelValue.type === 'faqCollection'">
      <label class="field"><span>区块标题</span>
        <input :value="modelValue.heading ?? ''" maxlength="200" @input="patch({ heading: ($event.target as HTMLInputElement).value || null })" />
      </label>
      <p class="inspector-copy">FAQ 条目来自「类型字段」，此区块只负责在页面上的呈现位置。</p>
    </template>
  </div>
</template>

<style scoped>
@layer components {
.block-fields { display: flex; flex-direction: column; gap: 0.65rem; }
.block-fields__list { display: flex; flex-direction: column; gap: 0.5rem; }
.block-fields__list-head { display: flex; align-items: center; justify-content: space-between; }
.block-fields__list-head strong { font-size: 0.75rem; }
.block-fields__item { display: flex; flex-direction: column; gap: 0.45rem; padding: 0.55rem; border: 1px solid var(--border-default); border-radius: 8px; background: white; }
.block-fields__item-head { display: flex; align-items: center; justify-content: space-between; }
.block-fields__item-head strong { color: var(--text-secondary); font-size: 0.75rem; letter-spacing: 0.05em; text-transform: uppercase; }
.block-fields__item-head > div { display: flex; gap: 0.15rem; }
.block-fields__channels { display: flex; flex-direction: column; gap: 0.35rem; padding: 0.5rem; margin: 0; border: 1px solid var(--border-default); border-radius: 8px; }
.block-fields__channels legend { padding: 0 0.3rem; color: var(--text-secondary); font-size: 0.75rem; }
}
</style>
