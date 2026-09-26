<script setup lang="ts">
import PublicBlockRenderer from '@/features/content/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

defineProps<{ page: PublicPageModel }>()
const { locale, t } = useI18n({ useScope: 'global' })
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section v-if="page.kind === 'home' && page.productFamilies?.length" class="section shell" aria-labelledby="database-product-families" :lang="locale">
      <h2 id="database-product-families">{{ t('content.productFamilies') }}</h2>
      <div class="family-strip">
        <a v-for="family in page.productFamilies" :key="family.code" :href="`/en/products?family=${family.code}`">
          <strong lang="en">{{ family.name }}</strong>
          <small v-if="family.description" lang="en">{{ family.description }}</small>
        </a>
      </div>
    </section>

  </main>
</template>
