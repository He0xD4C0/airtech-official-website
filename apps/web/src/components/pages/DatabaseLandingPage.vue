<script setup lang="ts">
import PublicBlockRenderer from '@/components/blocks/PublicBlockRenderer.vue'
import type { PublicPageModel } from '@/types/content'

defineProps<{ page: PublicPageModel }>()
</script>

<template>
  <main id="main-content">
    <PublicBlockRenderer
      v-if="page.projection"
      :blocks="page.projection.composition.blocks"
      :projection="page.projection"
      :breadcrumbs="page.breadcrumbs"
    />
    <section v-if="page.kind === 'home' && page.productFamilies?.length" class="section shell" aria-labelledby="database-product-families">
      <h2 id="database-product-families">Product families</h2>
      <div class="family-strip">
        <a v-for="family in page.productFamilies" :key="family.code" :href="`/en/products?family=${family.code}`">
          <strong>{{ family.name }}</strong>
          <small v-if="family.description">{{ family.description }}</small>
        </a>
      </div>
    </section>

  </main>
</template>
