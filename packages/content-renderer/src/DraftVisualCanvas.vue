<script setup lang="ts">
import BannerCarousel from './BannerCarousel.vue'
import { computed } from 'vue'
import type {
  ContentBlock,
  ContentDraftV2,
  EditorialAction,
  MediaUseReference,
} from '@airtek/contracts'

type MediaUrlMap = Readonly<Record<string, string | undefined>>

const props = withDefaults(defineProps<{
  document: ContentDraftV2
  mediaUrls?: MediaUrlMap
  pendingMediaUrls?: MediaUrlMap
}>(), {
  mediaUrls: () => ({}),
  pendingMediaUrls: () => ({}),
})

const blocks = computed(() => props.document.composition.blocks.filter(block => props.document.kind !== 'home' || block.type !== 'hero'))
const slides = computed(() => props.document.kind !== 'home' ? []
  : props.document.composition.blocks.flatMap(block => block.type === 'hero' ? [{
    id: block.id, eyebrow: block.eyebrow ?? '', heading: block.heading ?? props.document.title,
    lead: block.lead ?? props.document.summary ?? '', image: mediaUrl(block.media),
    alt: block.media?.decorative ? '' : block.media?.altText ?? '',
    actions: block.actions.map(action => ({ label: actionLabel(action), href: actionTarget(action) })),
  }] : []))

function bodyText(value: unknown): string {
  if (typeof value === 'string') return value
  if (Array.isArray(value)) return value.map(bodyText).filter(Boolean).join(' ')
  if (!value || typeof value !== 'object') return ''
  const node = value as { text?: unknown; content?: unknown }
  return [node.text, node.content].map(bodyText).filter(Boolean).join(' ')
}

function mediaUrl(media: MediaUseReference | null | undefined): string | undefined {
  if (!media) return undefined
  const id = media.asset.assetId
  return props.pendingMediaUrls[id] ?? props.mediaUrls[id]
}

function actionLabel(action: EditorialAction): string {
  return action.label.trim() || '链接'
}

function actionTarget(action: EditorialAction): string {
  const target = action.target
  if (target.targetType === 'external') return target.url
  if (target.targetType === 'route') return target.path
  return `#content-${target.contentId}`
}

function referenceTitle(block: ContentBlock): string {
  if (block.type === 'downloadAsset') return block.label || '下载资产'
  if (block.type === 'contactBlock') return block.heading || '联系方式'
  if (block.type === 'relationCollection') return block.heading || '关联内容'
  if (block.type === 'faqCollection') return block.heading || '常见问题'
  return block.type
}
</script>

<template>
  <article class="draft-canvas" aria-label="本地草稿视觉预览">
    <header class="draft-canvas__header">
      <p class="draft-canvas__eyebrow">{{ document.kind }} · {{ document.locale }}</p>
      <h1>{{ document.title || '未命名内容' }}</h1>
      <p v-if="document.summary" class="draft-canvas__lead">{{ document.summary }}</p>
    </header>

    <BannerCarousel :slides="slides" preview />
    <template v-for="block in blocks" :key="block.id">
      <section v-if="block.type === 'hero'" class="draft-canvas__hero">
        <div>
          <p v-if="block.eyebrow" class="draft-canvas__eyebrow">{{ block.eyebrow }}</p>
          <h2>{{ block.heading || document.title }}</h2>
          <p v-if="block.lead">{{ block.lead }}</p>
          <nav v-if="block.actions.length" class="draft-canvas__actions" aria-label="Hero 链接预览">
            <a v-for="action in block.actions" :key="action.label" :href="actionTarget(action)" @click.prevent>{{ actionLabel(action) }}</a>
          </nav>
        </div>
        <img v-if="mediaUrl(block.media)" :src="mediaUrl(block.media)" :alt="block.media?.decorative ? '' : block.media?.altText || ''" />
      </section>

      <section v-else-if="block.type === 'body'" class="draft-canvas__body">
        <p>{{ bodyText(document.body?.content) || '正文尚未填写。' }}</p>
      </section>

      <figure v-else-if="block.type === 'media'" class="draft-canvas__media">
        <img v-if="mediaUrl(block.media)" :src="mediaUrl(block.media)" :alt="block.media.decorative ? '' : block.media.altText || ''" />
        <div v-else class="draft-canvas__media-placeholder">媒体 {{ block.media.asset.assetId }}</div>
        <figcaption v-if="block.caption">{{ block.caption }}</figcaption>
      </figure>

      <section v-else-if="block.type === 'featureGrid'" class="draft-canvas__section">
        <h2 v-if="block.heading">{{ block.heading }}</h2>
        <div class="draft-canvas__grid">
          <article v-for="item in block.items" :key="item.id">
            <img v-if="mediaUrl(item.icon)" :src="mediaUrl(item.icon)" :alt="item.icon?.decorative ? '' : item.icon?.altText || ''" />
            <h3>{{ item.title }}</h3><p v-if="item.description">{{ item.description }}</p>
          </article>
        </div>
      </section>

      <section v-else-if="block.type === 'evidence'" class="draft-canvas__section">
        <h2 v-if="block.heading">{{ block.heading }}</h2>
        <dl><div v-for="item in block.items" :key="item.id"><dt>{{ item.label }}</dt><dd>{{ item.statement }}</dd></div></dl>
      </section>

      <section v-else-if="block.type === 'cta'" class="draft-canvas__cta">
        <p v-if="block.eyebrow" class="draft-canvas__eyebrow">{{ block.eyebrow }}</p>
        <h2>{{ block.heading }}</h2><p v-if="block.body">{{ block.body }}</p>
        <a :href="actionTarget(block.action)" @click.prevent>{{ actionLabel(block.action) }}</a>
      </section>

      <section v-else class="draft-canvas__reference">
        <strong>{{ referenceTitle(block) }}</strong>
        <p v-if="block.type === 'downloadAsset'">{{ block.description || '下载资产' }}</p>
        <p v-else-if="block.type === 'relationCollection'">{{ block.relationIds.length }} 个关联内容</p>
        <p v-else-if="block.type === 'contactBlock'">{{ block.channels.join(' · ') }}</p>
      </section>
    </template>
  </article>
</template>

<style scoped>
@layer components {
  .draft-canvas { overflow: hidden; border: 1px solid var(--airtek-border); border-radius: var(--airtek-radius-lg); background: var(--airtek-white); color: var(--airtek-ink); }
  .draft-canvas__header, .draft-canvas__section, .draft-canvas__body, .draft-canvas__cta, .draft-canvas__reference { padding: clamp(1.25rem, 4vw, 3rem); }
  .draft-canvas__header { background: linear-gradient(135deg, var(--airtek-blue-soft), var(--airtek-white)); }
  .draft-canvas h1 { max-width: 18ch; margin: 0; font-size: clamp(2rem, 5vw, 4rem); }
  .draft-canvas h2 { margin: 0 0 var(--space-3); font-size: clamp(1.4rem, 3vw, 2.2rem); }
  .draft-canvas h3 { margin: var(--space-2) 0; }
  .draft-canvas p { line-height: 1.7; }
  .draft-canvas__eyebrow { color: var(--airtek-blue); font-size: 0.75rem; font-weight: 800; letter-spacing: 0.12em; text-transform: uppercase; }
  .draft-canvas__lead { max-width: 48rem; font-size: 1.1rem; }
  .draft-canvas__hero { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 18rem), 1fr)); align-items: center; gap: var(--space-5); padding: clamp(1.25rem, 4vw, 3rem); background: var(--airtek-surface); }
  .draft-canvas__hero img, .draft-canvas__media img { display: block; width: 100%; height: auto; border-radius: var(--airtek-radius-md); }
  .draft-canvas__actions { display: flex; flex-wrap: wrap; gap: var(--space-3); }
  .draft-canvas__actions a, .draft-canvas__cta > a { display: inline-flex; min-height: 2.75rem; align-items: center; padding-inline: var(--space-4); border-radius: 999px; background: var(--airtek-blue); color: white; font-weight: 700; text-decoration: none; }
  .draft-canvas__media { margin: 0; padding: clamp(1.25rem, 4vw, 3rem); }
  .draft-canvas__media-placeholder { display: grid; min-height: 12rem; place-items: center; border: 1px dashed var(--airtek-border); color: var(--airtek-muted); overflow-wrap: anywhere; }
  .draft-canvas figcaption { margin-top: var(--space-2); color: var(--airtek-muted); font-size: 0.8rem; }
  .draft-canvas__grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr)); gap: var(--space-4); }
  .draft-canvas__grid article { padding: var(--space-4); border: 1px solid var(--airtek-border); border-radius: var(--airtek-radius-md); }
  .draft-canvas__grid img { width: 3rem; height: 3rem; object-fit: contain; }
  .draft-canvas dl { display: grid; gap: var(--space-3); margin: 0; }
  .draft-canvas dl div { padding-block: var(--space-3); border-bottom: 1px solid var(--airtek-border); }
  .draft-canvas dt { color: var(--airtek-muted); font-size: 0.75rem; font-weight: 700; }
  .draft-canvas dd { margin: var(--space-1) 0 0; }
  .draft-canvas__cta { background: var(--airtek-green-soft); }
  .draft-canvas__reference { border-top: 1px solid var(--airtek-border); }
}
</style>
