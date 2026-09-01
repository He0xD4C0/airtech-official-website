<script setup lang="ts">
import type { PublishedArticleMetadata } from '@/lib/publishedContent'

defineProps<{ metadata: PublishedArticleMetadata }>()
</script>

<template>
  <header v-if="metadata.author || metadata.publishedAt || metadata.category || metadata.updatedAt" class="article-publication-meta">
    <dl>
      <div v-if="metadata.author">
        <dt>Author</dt>
        <dd>{{ metadata.author }}</dd>
      </div>
      <div v-if="metadata.publishedAt && metadata.publishedDate">
        <dt>Published</dt>
        <dd><time :datetime="metadata.publishedAt">{{ metadata.publishedDate }}</time></dd>
      </div>
      <div v-if="metadata.updatedAt && metadata.updatedDate">
        <dt>Last updated</dt>
        <dd><time :datetime="metadata.updatedAt">{{ metadata.updatedDate }}</time></dd>
      </div>
      <div v-if="metadata.category">
        <dt>Category</dt>
        <dd>{{ metadata.category }}</dd>
      </div>
    </dl>
  </header>
  <nav v-if="metadata.outline.length" class="article-toc" aria-label="On this page">
    <p class="eyebrow">On this page</p>
    <ol>
      <li v-for="item in metadata.outline" :key="item.id" :class="`level-${item.level}`">
        <a :href="`#${item.id}`">{{ item.title }}</a>
      </li>
    </ol>
  </nav>
</template>
