<script setup lang="ts">
import type { ArticlePublicationMetadata } from '@/features/content/lib/articlePresentation'
import { useI18n } from 'vue-i18n'

defineProps<{ metadata: ArticlePublicationMetadata }>()
const { locale, t } = useI18n({ useScope: 'global' })

function formatDate(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value.slice(0, 10)
  return new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium', timeZone: 'UTC' }).format(date)
}
</script>

<template>
  <header v-if="metadata.author || metadata.publishedAt || metadata.category || metadata.updatedAt" class="article-publication-meta" :lang="locale">
    <dl>
      <div v-if="metadata.author">
        <dt>{{ t('content.author') }}</dt>
        <dd lang="en">{{ metadata.author }}</dd>
      </div>
      <div v-if="metadata.publishedAt && metadata.publishedDate">
        <dt>{{ t('content.published') }}</dt>
        <dd><time :datetime="metadata.publishedAt">{{ formatDate(metadata.publishedAt) }}</time></dd>
      </div>
      <div v-if="metadata.updatedAt && metadata.updatedDate">
        <dt>{{ t('content.lastUpdated') }}</dt>
        <dd><time :datetime="metadata.updatedAt">{{ formatDate(metadata.updatedAt) }}</time></dd>
      </div>
      <div v-if="metadata.category">
        <dt>{{ t('content.category') }}</dt>
        <dd lang="en">{{ metadata.category }}</dd>
      </div>
    </dl>
  </header>
  <nav v-if="metadata.outline.length" class="article-toc" :aria-label="t('content.onThisPage')" :lang="locale">
    <p class="eyebrow">{{ t('content.onThisPage') }}</p>
    <ol>
      <li v-for="item in metadata.outline" :key="item.id" :class="`level-${item.level}`">
        <a :href="`#${item.id}`" lang="en">{{ item.title }}</a>
      </li>
    </ol>
  </nav>
</template>
