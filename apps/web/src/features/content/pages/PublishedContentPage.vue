<script setup lang="ts">
import { computed } from 'vue'
import ArticlePublicationMeta from '@/features/content/components/content/ArticlePublicationMeta.vue'
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import { articleMetadataFrom } from '@/features/content/lib/publicProjectionV2'
import type { PublicPageModel } from '@/shared/types/content'

const props = defineProps<{ page: PublicPageModel }>()
const projectionMetadata = computed(() => (
  props.page.projection ? articleMetadataFrom(props.page.projection) : null
))

function openCookieSettings(): void {
  window.dispatchEvent(new CustomEvent('airtek:open-cookie-settings'))
}
</script>

<template>
  <main id="main-content">
    <template v-if="page.projection">
      <PublicBlockRenderer
        :blocks="page.projection.composition.blocks"
        :projection="page.projection"
        :breadcrumbs="page.breadcrumbs"
      />
      <section v-if="projectionMetadata" class="section shell published-content">
        <ArticlePublicationMeta
          v-if="page.projection.kind === 'article'"
          :metadata="projectionMetadata"
        />
      </section>
    </template>
    <section v-if="page.kind === 'legal' && page.canonicalPath.endsWith('/cookie-settings')" class="section shell published-cookie-control" aria-labelledby="published-cookie-title">
      <h2 id="published-cookie-title">Cookie preference</h2>
      <button class="button" type="button" @click="openCookieSettings">Review cookie preference</button>
    </section>
  </main>
</template>
